import React, { useEffect, useId, useState } from 'react';
import {
  AlertTriangle,
  Code2,
  FolderPlus,
  Layers,
  Plus,
  RotateCcw,
  Sparkles,
  SlidersHorizontal,
  Trash2,
  X,
} from 'lucide-react';
import {
  VISUAL_MAX_FILTER_DEPTH,
  getFilterTreeDepth,
  parseFilterNodeJson,
  type ArrayTextCondition,
  type ArrayTextField,
  type CatalogFacets,
  type FilterCondition,
  type FilterField,
  type FilterMediaType,
  type FilterNode,
  type IntFilterCondition,
  type IntFilterField,
  type LogicalOp,
  type ScalarTextCondition,
  type ScalarTextField,
  type TimeFilterCondition,
  type TimeFilterField,
  type WatchStateFilterCondition,
  type WatchStateValue,
} from '../../types/filterAst';
import type { Library } from '../../types';

export interface RuleBuilderEditorProps {
  value: FilterNode | null | undefined;
  onChange: (next: FilterNode | null) => void;
  libraries?: Library[];
  facets?: CatalogFacets | null;
  onValidationErrorChange?: (error: string | null) => void;
}

interface QuickTemplate {
  id: string;
  label: string;
  build: () => FilterNode;
}

const QUICK_TEMPLATES: QuickTemplate[] = [
  {
    id: 'short-films',
    label: 'Short Films (< 90 min)',
    build: () => ({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: { field: 'item_type', op: 'in', values: ['movie'] },
        },
        {
          kind: 'rule',
          condition: { field: 'duration_seconds', op: 'lte', value: 5400 },
        },
      ],
    }),
  },
  {
    id: 'unwatched-gems',
    label: 'Unwatched Gems (8+)',
    build: () => ({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: { field: 'watch_state', op: 'state_in', values: ['unwatched'] },
        },
        {
          kind: 'rule',
          condition: { field: 'rating', op: 'gte', value: 8 },
        },
      ],
    }),
  },
  {
    id: '90s-classics',
    label: '90s Classics',
    build: () => ({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: { field: 'item_type', op: 'in', values: ['movie'] },
        },
        {
          kind: 'rule',
          condition: { field: 'release_year', op: 'between', min: 1990, max: 1999 },
        },
      ],
    }),
  },
  {
    id: '4k-showcase',
    label: '4K Showcase',
    build: () => ({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'resolution',
            op: 'in',
            values: ['2160p', '4K', '3840x2160'],
          },
        },
      ],
    }),
  },
  {
    id: 'fresh-arrivals',
    label: 'Fresh Arrivals (30d)',
    build: () => ({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: { field: 'added_at', op: 'within_last_days', days: 30 },
        },
        {
          kind: 'rule',
          condition: { field: 'watch_state', op: 'state_in', values: ['unwatched'] },
        },
      ],
    }),
  },
  {
    id: 'almost-finished',
    label: 'Almost Finished',
    build: () => ({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: { field: 'watch_state', op: 'progress_between', min: 0.7, max: 0.98 },
        },
      ],
    }),
  },
];

const FIELD_GROUPS: { group: string; options: { value: FilterField; label: string }[] }[] = [
  {
    group: 'Scope',
    options: [
      { value: 'item_type', label: 'Media Type' },
      { value: 'library_id', label: 'Library' },
    ],
  },
  {
    group: 'Metadata',
    options: [
      { value: 'title', label: 'Title' },
      { value: 'original_title', label: 'Original Title' },
      { value: 'series_title', label: 'Series Title' },
      { value: 'genres', label: 'Genres' },
      { value: 'tags', label: 'Tags' },
      { value: 'release_year', label: 'Release Year' },
      { value: 'rating', label: 'Rating (0–10)' },
      { value: 'country', label: 'Country' },
      { value: 'language', label: 'Language' },
      { value: 'studio', label: 'Studio' },
    ],
  },
  {
    group: 'People',
    options: [
      { value: 'director', label: 'Director' },
      { value: 'actors', label: 'Actors' },
      { value: 'writers', label: 'Writers' },
    ],
  },
  {
    group: 'Technical',
    options: [
      { value: 'duration_seconds', label: 'Duration (minutes)' },
      { value: 'resolution', label: 'Resolution' },
      { value: 'video_codec', label: 'Video Codec' },
      { value: 'audio_codec', label: 'Audio Codec' },
      { value: 'audio_channels', label: 'Audio Channels' },
      { value: 'container', label: 'Container' },
    ],
  },
  {
    group: 'Activity & Playback',
    options: [
      { value: 'watch_state', label: 'Watch State' },
      { value: 'play_count', label: 'Play Count' },
      { value: 'added_at', label: 'Date Added' },
      { value: 'last_watched_at', label: 'Last Watched' },
    ],
  },
];

const MEDIA_TYPE_OPTIONS: { value: FilterMediaType; label: string }[] = [
  { value: 'movie', label: 'Movie' },
  { value: 'show', label: 'Show' },
  { value: 'season', label: 'Season' },
  { value: 'episode', label: 'Episode' },
  { value: 'anime', label: 'Anime' },
  { value: 'music', label: 'Music' },
  { value: 'home_videos', label: 'Home Videos' },
  { value: 'audiobook', label: 'Audiobook' },
  { value: 'live_channel', label: 'Live Channel' },
];

const WATCH_STATE_OPTIONS: { value: WatchStateValue; label: string }[] = [
  { value: 'unwatched', label: 'Unwatched' },
  { value: 'in_progress', label: 'In Progress' },
  { value: 'completed', label: 'Completed' },
];

const SCALAR_TEXT_OPS = [
  { value: 'eq', label: 'Equals' },
  { value: 'contains', label: 'Contains' },
  { value: 'starts_with', label: 'Starts With' },
  { value: 'in', label: 'Is Any Of' },
  { value: 'is_empty', label: 'Is Empty' },
] as const;

const ARRAY_TEXT_OPS = [
  { value: 'contains_any', label: 'Contains Any' },
  { value: 'contains_all', label: 'Contains All' },
  { value: 'is_empty', label: 'Is Empty' },
] as const;

const INT_OPS = [
  { value: 'eq', label: 'Equals (=)' },
  { value: 'gte', label: 'At Least (>=)' },
  { value: 'lte', label: 'At Most (<=)' },
  { value: 'between', label: 'Between' },
] as const;

const FLOAT_OPS = [
  { value: 'gte', label: 'At Least (>=)' },
  { value: 'lte', label: 'At Most (<=)' },
  { value: 'between', label: 'Between' },
] as const;

const TIME_OPS = [
  { value: 'within_last_days', label: 'Within Last N Days' },
  { value: 'older_than_days', label: 'Older Than N Days' },
  { value: 'after_epoch', label: 'On or After Date' },
  { value: 'before_epoch', label: 'On or Before Date' },
  { value: 'between_epochs', label: 'Between Dates' },
] as const;

const WATCH_STATE_OPS = [
  { value: 'state_in', label: 'State Is' },
  { value: 'progress_between', label: 'Progress Between (%)' },
] as const;

function isScalarTextField(field: FilterField): field is ScalarTextField {
  return (
    field === 'title' ||
    field === 'original_title' ||
    field === 'series_title' ||
    field === 'director' ||
    field === 'studio' ||
    field === 'country' ||
    field === 'language' ||
    field === 'resolution' ||
    field === 'video_codec' ||
    field === 'audio_codec' ||
    field === 'container'
  );
}

function isArrayTextField(field: FilterField): field is ArrayTextField {
  return field === 'genres' || field === 'tags' || field === 'actors' || field === 'writers';
}

function isIntField(field: FilterField): field is IntFilterField {
  return (
    field === 'release_year' ||
    field === 'duration_seconds' ||
    field === 'audio_channels' ||
    field === 'play_count'
  );
}

function isTimeField(field: FilterField): field is TimeFilterField {
  return field === 'added_at' || field === 'last_watched_at';
}

function isScalarTextCondition(cond: FilterCondition): cond is ScalarTextCondition {
  return isScalarTextField(cond.field);
}

function isArrayTextCondition(cond: FilterCondition): cond is ArrayTextCondition {
  return isArrayTextField(cond.field);
}

function isIntCondition(cond: FilterCondition): cond is IntFilterCondition {
  return isIntField(cond.field);
}

function isTimeCondition(cond: FilterCondition): cond is TimeFilterCondition {
  return isTimeField(cond.field);
}

function isWatchStateCondition(cond: FilterCondition): cond is WatchStateFilterCondition {
  return cond.field === 'watch_state';
}

function getFacetSuggestions(
  field: FilterField,
  facets?: CatalogFacets | null
): string[] {
  if (!facets) return [];
  switch (field) {
    case 'genres':
      return facets.genres || [];
    case 'country':
      return facets.countries || [];
    case 'language':
      return facets.languages || [];
    case 'studio':
      return facets.studios || [];
    case 'director':
      return facets.directors || [];
    case 'tags':
      return facets.tags || [];
    case 'resolution':
      return facets.resolutions || [];
    case 'video_codec':
      return facets.video_codecs || [];
    case 'audio_codec':
      return facets.audio_codecs || [];
    case 'container':
      return facets.containers || [];
    default:
      return [];
  }
}

function createDefaultRule(): Extract<FilterNode, { kind: 'rule' }> {
  return {
    kind: 'rule',
    condition: {
      field: 'item_type',
      op: 'in',
      values: ['movie'],
    },
  };
}

function createDefaultConditionForField(
  field: FilterField,
  libraries: Library[],
  facets?: CatalogFacets | null
): FilterCondition {
  if (field === 'library_id') {
    return {
      field: 'library_id',
      op: 'in',
      values: libraries[0]?.id ? [libraries[0].id] : [],
    };
  }
  if (field === 'item_type') {
    return { field: 'item_type', op: 'in', values: ['movie'] };
  }
  if (isScalarTextField(field)) {
    const suggestions = getFacetSuggestions(field, facets);
    return {
      field,
      op: 'eq',
      value: suggestions[0] ?? '',
    };
  }
  if (isArrayTextField(field)) {
    const suggestions = getFacetSuggestions(field, facets);
    return {
      field,
      op: 'contains_any',
      values: suggestions[0] ? [suggestions[0]] : [],
    };
  }
  if (isIntField(field)) {
    if (field === 'duration_seconds') {
      return { field: 'duration_seconds', op: 'lte', value: 5400 };
    }
    if (field === 'release_year') {
      return {
        field: 'release_year',
        op: 'gte',
        value: facets?.year_min ?? 2000,
      };
    }
    if (field === 'audio_channels') {
      return { field: 'audio_channels', op: 'gte', value: 6 };
    }
    return { field: 'play_count', op: 'gte', value: 1 };
  }
  if (field === 'rating') {
    return { field: 'rating', op: 'gte', value: 7.5 };
  }
  if (isTimeField(field)) {
    return { field, op: 'within_last_days', days: 30 };
  }
  return {
    field: 'watch_state',
    op: 'state_in',
    values: ['unwatched'],
  };
}

function convertConditionOperator(
  condition: FilterCondition,
  nextOp: string
): FilterCondition {
  const field = condition.field;

  if (field === 'library_id' || field === 'item_type') {
    return condition;
  }

  if (isScalarTextCondition(condition)) {
    const field = condition.field;
    const prevSingle =
      condition.op === 'eq' || condition.op === 'contains' || condition.op === 'starts_with'
        ? condition.value
        : condition.op === 'in' && condition.values.length > 0
          ? condition.values[0]
          : '';
    if (nextOp === 'eq' || nextOp === 'contains' || nextOp === 'starts_with') {
      return { field, op: nextOp, value: prevSingle };
    }
    if (nextOp === 'in') {
      const vals =
        condition.op === 'in'
          ? condition.values
          : prevSingle
            ? [prevSingle]
            : [];
      return { field, op: 'in', values: vals };
    }
    return { field, op: 'is_empty' };
  }

  if (isArrayTextCondition(condition)) {
    const field = condition.field;
    const prevVals = condition.op === 'is_empty' ? [] : condition.values;
    if (nextOp === 'contains_any' || nextOp === 'contains_all') {
      return { field, op: nextOp, values: prevVals };
    }
    return { field, op: 'is_empty' };
  }

  if (isIntCondition(condition)) {
    const field = condition.field;
    const prevVal = condition.op === 'between' ? condition.min : condition.value;
    if (nextOp === 'eq' || nextOp === 'gte' || nextOp === 'lte') {
      return { field, op: nextOp, value: prevVal };
    }
    const maxVal =
      condition.op === 'between'
        ? condition.max
        : field === 'duration_seconds'
          ? Math.max(prevVal, prevVal + 1800)
          : prevVal + 10;
    return { field, op: 'between', min: prevVal, max: maxVal };
  }

  if (condition.field === 'rating') {
    const prevVal = condition.op === 'between' ? condition.min : condition.value;
    if (nextOp === 'gte' || nextOp === 'lte') {
      return { field: 'rating', op: nextOp, value: prevVal };
    }
    return {
      field: 'rating',
      op: 'between',
      min: prevVal,
      max: Math.min(10, Math.max(prevVal, 10)),
    };
  }

  if (isTimeCondition(condition)) {
    const field = condition.field;
    if (nextOp === 'within_last_days' || nextOp === 'older_than_days') {
      const days =
        condition.op === 'within_last_days' || condition.op === 'older_than_days'
          ? condition.days
          : 30;
      return { field, op: nextOp, days };
    }
    const defaultEpoch = 1704067200; // 2024-01-01T00:00:00Z
    if (nextOp === 'after_epoch' || nextOp === 'before_epoch') {
      const ts =
        condition.op === 'after_epoch' || condition.op === 'before_epoch'
          ? condition.timestamp
          : condition.op === 'between_epochs'
            ? condition.min
            : defaultEpoch;
      return { field, op: nextOp, timestamp: ts };
    }
    return {
      field,
      op: 'between_epochs',
      min: defaultEpoch,
      max: defaultEpoch + 86400 * 365,
    };
  }

  if (nextOp === 'progress_between') {
    return { field: 'watch_state', op: 'progress_between', min: 0.1, max: 0.9 };
  }
  return { field: 'watch_state', op: 'state_in', values: ['unwatched'] };
}

function epochToDateInput(epochSeconds: number): string {
  if (!Number.isFinite(epochSeconds) || epochSeconds <= 0) return '2024-01-01';
  return new Date(epochSeconds * 1000).toISOString().slice(0, 10);
}

function dateInputToEpoch(dateStr: string): number {
  const ms = Date.parse(`${dateStr}T00:00:00Z`);
  return Number.isFinite(ms) ? Math.floor(ms / 1000) : 0;
}

function buildGroupNode(
  op: LogicalOp,
  negated: boolean,
  children: FilterNode[]
): Extract<FilterNode, { kind: 'group' }> {
  return {
    kind: 'group',
    op,
    ...(negated ? { negated: true } : {}),
    children,
  };
}

function buildRuleNode(
  negated: boolean,
  condition: FilterCondition
): Extract<FilterNode, { kind: 'rule' }> {
  return {
    kind: 'rule',
    ...(negated ? { negated: true } : {}),
    condition,
  };
}

function normalizeVisualRoot(
  value: FilterNode | null | undefined,
  fallbackOp: LogicalOp,
  fallbackNegated: boolean
): Extract<FilterNode, { kind: 'group' }> | null {
  if (!value) return null;
  if (value.kind === 'group') return value;
  return buildGroupNode(fallbackOp, fallbackNegated, [value]);
}

interface MultiChipEditorProps {
  labelPrefix: string;
  values: string[];
  suggestions: string[];
  datalistId: string;
  onChangeValues: (next: string[]) => void;
}

const MultiChipEditor: React.FC<MultiChipEditorProps> = ({
  labelPrefix,
  values,
  suggestions,
  datalistId,
  onChangeValues,
}) => {
  const [draft, setDraft] = useState('');

  const addToken = (raw: string) => {
    const token = raw.trim();
    if (!token) return;
    const exists = values.some((v) => v.toLowerCase() === token.toLowerCase());
    if (!exists) {
      onChangeValues([...values, token]);
    }
    setDraft('');
  };

  const toggleToken = (token: string) => {
    const exists = values.some((v) => v.toLowerCase() === token.toLowerCase());
    if (exists) {
      onChangeValues(values.filter((v) => v.toLowerCase() !== token.toLowerCase()));
    } else {
      onChangeValues([...values, token]);
    }
  };

  return (
    <div className="space-y-2 flex-1 min-w-[180px]">
      <div className="flex flex-wrap items-center gap-1.5 bg-canvas border border-border-subtle rounded-xl px-2.5 py-1.5">
        {values.map((val) => (
          <span
            key={val}
            className="inline-flex items-center gap-1 bg-accent/20 text-accent text-xs font-medium px-2 py-0.5 rounded-lg"
          >
            {val}
            <button
              type="button"
              aria-label={`Remove ${val}`}
              onClick={() =>
                onChangeValues(values.filter((item) => item !== val))
              }
              className="hover:text-text-main cursor-pointer focus-visible:ring-2 focus-visible:ring-highlight focus-visible:outline-none rounded"
            >
              <X className="w-3 h-3" aria-hidden="true" />
            </button>
          </span>
        ))}
        <input
          type="text"
          aria-label={`${labelPrefix} value`}
          list={suggestions.length > 0 ? datalistId : undefined}
          value={draft}
          placeholder={values.length === 0 ? 'Add value & press Enter...' : 'Add more...'}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return;
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault();
              addToken(draft);
            }
          }}
          onBlur={() => {
            if (draft.trim()) {
              addToken(draft);
            }
          }}
          className="flex-1 min-w-[110px] bg-transparent text-xs text-text-main placeholder:text-muted/60 py-1 focus:outline-none"
        />
      </div>
      {suggestions.length > 0 && (
        <>
          <datalist id={datalistId}>
            {suggestions.map((opt) => (
              <option key={opt} value={opt} />
            ))}
          </datalist>
          <div className="flex flex-wrap gap-1">
            {suggestions.slice(0, 10).map((opt) => {
              const selected = values.some((v) => v.toLowerCase() === opt.toLowerCase());
              return (
                <button
                  key={opt}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => toggleToken(opt)}
                  className={`px-2 py-0.5 rounded-lg text-[11px] font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                    selected
                      ? 'bg-accent text-canvas font-semibold'
                      : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                  }`}
                >
                  {opt}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
};

interface RuleRowProps {
  rule: Extract<FilterNode, { kind: 'rule' }>;
  labelPrefix: string;
  libraries: Library[];
  facets?: CatalogFacets | null;
  onUpdate: (next: Extract<FilterNode, { kind: 'rule' }>) => void;
  onRemove: () => void;
}

const RuleRow: React.FC<RuleRowProps> = ({
  rule,
  labelPrefix,
  libraries,
  facets,
  onUpdate,
  onRemove,
}) => {
  const reactId = useId();
  const datalistId = `facet-list-${reactId.replace(/:/g, '')}`;
  const { condition } = rule;
  const negated = Boolean(rule.negated);
  const suggestions = getFacetSuggestions(condition.field, facets);

  const updateCondition = (nextCond: FilterCondition) => {
    onUpdate(buildRuleNode(negated, nextCond));
  };

  const renderOperatorSelect = () => {
    const field = condition.field;
    let ops: readonly { value: string; label: string }[] = [];
    if (field === 'library_id' || field === 'item_type') {
      ops = [{ value: 'in', label: 'Is Any Of' }];
    } else if (isScalarTextField(field)) {
      ops = SCALAR_TEXT_OPS;
    } else if (isArrayTextField(field)) {
      ops = ARRAY_TEXT_OPS;
    } else if (isIntField(field)) {
      ops = INT_OPS;
    } else if (field === 'rating') {
      ops = FLOAT_OPS;
    } else if (isTimeField(field)) {
      ops = TIME_OPS;
    } else if (field === 'watch_state') {
      ops = WATCH_STATE_OPS;
    }

    return (
      <select
        aria-label={`${labelPrefix} operator`}
        value={condition.op}
        onChange={(e) => updateCondition(convertConditionOperator(condition, e.target.value))}
        className="bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
      >
        {ops.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    );
  };

  const renderValueControl = () => {
    if (condition.field === 'library_id') {
      if (libraries.length === 0) {
        return (
          <MultiChipEditor
            labelPrefix={labelPrefix}
            values={condition.values}
            suggestions={[]}
            datalistId={datalistId}
            onChangeValues={(vals) =>
              updateCondition({ field: 'library_id', op: 'in', values: vals })
            }
          />
        );
      }
      return (
        <div className="flex flex-wrap gap-1.5 flex-1">
          {libraries.map((lib) => {
            const active = condition.values.includes(lib.id);
            return (
              <button
                key={lib.id}
                type="button"
                aria-pressed={active}
                onClick={() => {
                  const next = active
                    ? condition.values.filter((id) => id !== lib.id)
                    : [...condition.values, lib.id];
                  updateCondition({ field: 'library_id', op: 'in', values: next });
                }}
                className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  active
                    ? 'bg-accent text-canvas font-semibold'
                    : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                }`}
              >
                {lib.name}
              </button>
            );
          })}
        </div>
      );
    }

    if (condition.field === 'item_type') {
      return (
        <div className="flex flex-wrap gap-1.5 flex-1">
          {MEDIA_TYPE_OPTIONS.map((opt) => {
            const active = condition.values.includes(opt.value);
            return (
              <button
                key={opt.value}
                type="button"
                aria-pressed={active}
                onClick={() => {
                  const next = active
                    ? condition.values.filter((t) => t !== opt.value)
                    : [...condition.values, opt.value];
                  updateCondition({
                    field: 'item_type',
                    op: 'in',
                    values: next.length > 0 ? next : [opt.value],
                  });
                }}
                className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  active
                    ? 'bg-accent text-canvas font-semibold'
                    : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      );
    }

    if (isScalarTextCondition(condition)) {
      if (condition.op === 'is_empty') {
        return (
          <span className="text-xs text-muted italic px-2 py-1.5">
            Matches items with missing or blank value
          </span>
        );
      }
      if (condition.op === 'in') {
        return (
          <MultiChipEditor
            labelPrefix={labelPrefix}
            values={condition.values}
            suggestions={suggestions}
            datalistId={datalistId}
            onChangeValues={(vals) =>
              updateCondition({ field: condition.field, op: 'in', values: vals })
            }
          />
        );
      }
      return (
        <div className="space-y-1.5 flex-1 min-w-[180px]">
          <input
            type="text"
            aria-label={`${labelPrefix} value`}
            list={suggestions.length > 0 ? datalistId : undefined}
            value={condition.value}
            placeholder="Enter value..."
            onChange={(e) =>
              updateCondition({
                field: condition.field,
                op: condition.op,
                value: e.target.value,
              })
            }
            className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-xs text-text-main placeholder:text-muted/60 transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
          {suggestions.length > 0 && (
            <>
              <datalist id={datalistId}>
                {suggestions.map((opt) => (
                  <option key={opt} value={opt} />
                ))}
              </datalist>
              <div className="flex flex-wrap gap-1">
                {suggestions.slice(0, 10).map((opt) => {
                  const selected = condition.value.toLowerCase() === opt.toLowerCase();
                  return (
                    <button
                      key={opt}
                      type="button"
                      aria-pressed={selected}
                      onClick={() =>
                        updateCondition({
                          field: condition.field,
                          op: condition.op,
                          value: opt,
                        })
                      }
                      className={`px-2 py-0.5 rounded-lg text-[11px] font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                        selected
                          ? 'bg-accent text-canvas font-semibold'
                          : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                      }`}
                    >
                      {opt}
                    </button>
                  );
                })}
              </div>
            </>
          )}
        </div>
      );
    }

    if (isArrayTextCondition(condition)) {
      if (condition.op === 'is_empty') {
        return (
          <span className="text-xs text-muted italic px-2 py-1.5">
            Matches items with an empty list
          </span>
        );
      }
      return (
        <MultiChipEditor
          labelPrefix={labelPrefix}
          values={condition.values}
          suggestions={suggestions}
          datalistId={datalistId}
          onChangeValues={(vals) =>
            updateCondition({
              field: condition.field,
              op: condition.op,
              values: vals,
            })
          }
        />
      );
    }

    if (isIntCondition(condition)) {
      const isDuration = condition.field === 'duration_seconds';
      if (condition.op === 'between') {
        const displayMin = isDuration
          ? Math.round((condition.min / 60) * 100) / 100
          : condition.min;
        const displayMax = isDuration
          ? Math.round((condition.max / 60) * 100) / 100
          : condition.max;
        return (
          <div className="flex items-center gap-2 flex-1">
            <input
              type="number"
              min={isDuration ? 0 : undefined}
              aria-label={isDuration ? `${labelPrefix} min (minutes)` : `${labelPrefix} min`}
              value={displayMin}
              onChange={(e) => {
                const raw = Number(e.target.value) || 0;
                const nextMin = isDuration ? Math.max(0, Math.round(raw * 60)) : Math.round(raw);
                updateCondition({
                  field: condition.field,
                  op: 'between',
                  min: nextMin,
                  max: condition.max,
                });
              }}
              className="w-24 bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <span className="text-xs text-muted">to</span>
            <input
              type="number"
              min={isDuration ? 0 : undefined}
              aria-label={isDuration ? `${labelPrefix} max (minutes)` : `${labelPrefix} max`}
              value={displayMax}
              onChange={(e) => {
                const raw = Number(e.target.value) || 0;
                const nextMax = isDuration ? Math.max(0, Math.round(raw * 60)) : Math.round(raw);
                updateCondition({
                  field: condition.field,
                  op: 'between',
                  min: condition.min,
                  max: nextMax,
                });
              }}
              className="w-24 bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            {isDuration && <span className="text-xs text-muted">min</span>}
          </div>
        );
      }

      const displayValue = isDuration
        ? Math.round((condition.value / 60) * 100) / 100
        : condition.value;
      return (
        <div className="flex items-center gap-2 flex-1">
          <input
            type="number"
            min={isDuration || condition.field !== 'release_year' ? 0 : undefined}
            aria-label={isDuration ? `${labelPrefix} value (minutes)` : `${labelPrefix} value`}
            value={displayValue}
            onChange={(e) => {
              const raw = Number(e.target.value) || 0;
              const nextVal = isDuration ? Math.max(0, Math.round(raw * 60)) : Math.round(raw);
              updateCondition({
                field: condition.field,
                op: condition.op,
                value: nextVal,
              });
            }}
            className="w-28 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
          {isDuration && <span className="text-xs text-muted">min</span>}
          {condition.field === 'release_year' &&
            facets?.year_min != null &&
            facets?.year_max != null && (
              <span className="text-[11px] text-muted">
                ({facets.year_min}–{facets.year_max})
              </span>
            )}
        </div>
      );
    }

    if (condition.field === 'rating') {
      if (condition.op === 'between') {
        return (
          <div className="flex items-center gap-2 flex-1">
            <input
              type="number"
              step="0.1"
              min="0"
              max="10"
              aria-label={`${labelPrefix} min`}
              value={condition.min}
              onChange={(e) =>
                updateCondition({
                  field: 'rating',
                  op: 'between',
                  min: Math.min(10, Math.max(0, Number(e.target.value) || 0)),
                  max: condition.max,
                })
              }
              className="w-24 bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <span className="text-xs text-muted">to</span>
            <input
              type="number"
              step="0.1"
              min="0"
              max="10"
              aria-label={`${labelPrefix} max`}
              value={condition.max}
              onChange={(e) =>
                updateCondition({
                  field: 'rating',
                  op: 'between',
                  min: condition.min,
                  max: Math.min(10, Math.max(0, Number(e.target.value) || 0)),
                })
              }
              className="w-24 bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
          </div>
        );
      }
      return (
        <input
          type="number"
          step="0.1"
          min="0"
          max="10"
          aria-label={`${labelPrefix} value`}
          value={condition.value}
          onChange={(e) =>
            updateCondition({
              field: 'rating',
              op: condition.op,
              value: Math.min(10, Math.max(0, Number(e.target.value) || 0)),
            })
          }
          className="w-28 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        />
      );
    }

    if (isTimeCondition(condition)) {
      if (condition.op === 'within_last_days' || condition.op === 'older_than_days') {
        return (
          <div className="flex items-center gap-2 flex-1">
            <input
              type="number"
              min="1"
              max="36500"
              aria-label={`${labelPrefix} days`}
              value={condition.days}
              onChange={(e) =>
                updateCondition({
                  field: condition.field,
                  op: condition.op,
                  days: Math.max(1, Math.round(Number(e.target.value) || 1)),
                })
              }
              className="w-28 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <span className="text-xs text-muted">days</span>
          </div>
        );
      }
      if (condition.op === 'after_epoch' || condition.op === 'before_epoch') {
        return (
          <input
            type="date"
            aria-label={`${labelPrefix} date`}
            value={epochToDateInput(condition.timestamp)}
            onChange={(e) =>
              updateCondition({
                field: condition.field,
                op: condition.op,
                timestamp: dateInputToEpoch(e.target.value),
              })
            }
            className="bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
        );
      }
      if (condition.op !== 'between_epochs') {
        return null;
      }
      const { field: timeField, min: epochMin, max: epochMax } = condition;
      return (
        <div className="flex items-center gap-2 flex-1 flex-wrap">
          <input
            type="date"
            aria-label={`${labelPrefix} start date`}
            value={epochToDateInput(epochMin)}
            onChange={(e) =>
              updateCondition({
                field: timeField,
                op: 'between_epochs',
                min: dateInputToEpoch(e.target.value),
                max: epochMax,
              })
            }
            className="bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
          <span className="text-xs text-muted">to</span>
          <input
            type="date"
            aria-label={`${labelPrefix} end date`}
            value={epochToDateInput(epochMax)}
            onChange={(e) =>
              updateCondition({
                field: timeField,
                op: 'between_epochs',
                min: epochMin,
                max: dateInputToEpoch(e.target.value),
              })
            }
            className="bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
        </div>
      );
    }

    if (isWatchStateCondition(condition) && condition.op === 'progress_between') {
      const minPct = Math.round(condition.min * 100);
      const maxPct = Math.round(condition.max * 100);
      return (
        <div className="flex items-center gap-2 flex-1">
          <input
            type="number"
            min="0"
            max="100"
            aria-label={`${labelPrefix} min (%)`}
            value={minPct}
            onChange={(e) => {
              const clamped = Math.min(100, Math.max(0, Number(e.target.value) || 0));
              updateCondition({
                field: 'watch_state',
                op: 'progress_between',
                min: Number((clamped / 100).toFixed(4)),
                max: condition.max,
              });
            }}
            className="w-20 bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
          <span className="text-xs text-muted">% to</span>
          <input
            type="number"
            min="0"
            max="100"
            aria-label={`${labelPrefix} max (%)`}
            value={maxPct}
            onChange={(e) => {
              const clamped = Math.min(100, Math.max(0, Number(e.target.value) || 0));
              updateCondition({
                field: 'watch_state',
                op: 'progress_between',
                min: condition.min,
                max: Number((clamped / 100).toFixed(4)),
              });
            }}
            className="w-20 bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs text-text-main focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
          <span className="text-xs text-muted">%</span>
        </div>
      );
    }

    if (!isWatchStateCondition(condition) || condition.op !== 'state_in') {
      return null;
    }

    return (
      <div className="flex flex-wrap gap-1.5 flex-1">
        {WATCH_STATE_OPTIONS.map((opt) => {
          const active = condition.values.includes(opt.value);
          return (
            <button
              key={opt.value}
              type="button"
              aria-pressed={active}
              onClick={() => {
                const next = active
                  ? condition.values.filter((s) => s !== opt.value)
                  : [...condition.values, opt.value];
                updateCondition({
                  field: 'watch_state',
                  op: 'state_in',
                  values: next.length > 0 ? next : [opt.value],
                });
              }}
              className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                active
                  ? 'bg-accent text-canvas font-semibold'
                  : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
              }`}
            >
              {opt.label}
            </button>
          );
        })}
      </div>
    );
  };

  return (
    <div className="bg-panel/80 border border-border-subtle rounded-xl p-3 space-y-2">
      <div className="flex flex-wrap items-start gap-2">
        <button
          type="button"
          aria-label={`NOT ${labelPrefix.toLowerCase()}`}
          aria-pressed={negated}
          onClick={() => onUpdate(buildRuleNode(!negated, condition))}
          className={`px-2 py-2 rounded-xl text-xs font-bold tracking-wide transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
            negated
              ? 'bg-cta text-white'
              : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
          }`}
        >
          NOT
        </button>

        <select
          aria-label={`${labelPrefix} field`}
          value={condition.field}
          onChange={(e) =>
            updateCondition(
              createDefaultConditionForField(
                e.target.value as FilterField,
                libraries,
                facets
              )
            )
          }
          className="bg-canvas border border-border-subtle rounded-xl px-2.5 py-2 text-xs font-medium text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          {FIELD_GROUPS.map((grp) => (
            <optgroup key={grp.group} label={grp.group}>
              {grp.options.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </optgroup>
          ))}
        </select>

        {renderOperatorSelect()}

        <div className="flex-1 min-w-[180px] flex items-start">
          {renderValueControl()}
        </div>

        <button
          type="button"
          aria-label={`Remove ${labelPrefix.toLowerCase()}`}
          onClick={onRemove}
          className="p-2 text-muted hover:text-cta hover:bg-canvas rounded-xl transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
        </button>
      </div>
    </div>
  );
};

interface GroupCardProps {
  group: Extract<FilterNode, { kind: 'group' }>;
  groupIndex: number;
  libraries: Library[];
  facets?: CatalogFacets | null;
  onUpdate: (next: Extract<FilterNode, { kind: 'group' }>) => void;
  onRemoveGroup: () => void;
}

const GroupCard: React.FC<GroupCardProps> = ({
  group,
  groupIndex,
  libraries,
  facets,
  onUpdate,
  onRemoveGroup,
}) => {
  const negated = Boolean(group.negated);
  const groupLabel = `Group ${groupIndex + 1}`;

  return (
    <div className="bg-canvas/70 border border-accent/30 rounded-xl p-3 space-y-2.5">
      <div className="flex flex-wrap items-center justify-between gap-2 pb-2 border-b border-border-subtle">
        <div className="flex items-center gap-1.5">
          <Layers className="w-3.5 h-3.5 text-accent" aria-hidden="true" />
          <span className="text-xs font-semibold text-text-main">{groupLabel}</span>
          <div className="inline-flex rounded-lg bg-panel p-0.5 border border-border-subtle ml-1">
            <button
              type="button"
              aria-label={`ALL (AND) ${groupLabel.toLowerCase()}`}
              aria-pressed={group.op === 'and'}
              onClick={() => onUpdate(buildGroupNode('and', negated, group.children))}
              className={`px-2 py-0.5 rounded-md text-[11px] font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                group.op === 'and'
                  ? 'bg-accent text-canvas'
                  : 'text-muted hover:text-text-main'
              }`}
            >
              ALL (AND)
            </button>
            <button
              type="button"
              aria-label={`ANY (OR) ${groupLabel.toLowerCase()}`}
              aria-pressed={group.op === 'or'}
              onClick={() => onUpdate(buildGroupNode('or', negated, group.children))}
              className={`px-2 py-0.5 rounded-md text-[11px] font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                group.op === 'or'
                  ? 'bg-accent text-canvas'
                  : 'text-muted hover:text-text-main'
              }`}
            >
              ANY (OR)
            </button>
          </div>
          <button
            type="button"
            aria-label={`NOT ${groupLabel.toLowerCase()}`}
            aria-pressed={negated}
            onClick={() => onUpdate(buildGroupNode(group.op, !negated, group.children))}
            className={`px-2 py-0.5 rounded-lg text-[11px] font-bold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
              negated
                ? 'bg-cta text-white'
                : 'bg-panel border border-border-subtle text-muted hover:text-text-main'
            }`}
          >
            NOT
          </button>
        </div>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            aria-label={`+ Add Rule to ${groupLabel.toLowerCase()}`}
            onClick={() =>
              onUpdate(
                buildGroupNode(group.op, negated, [...group.children, createDefaultRule()])
              )
            }
            className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium bg-panel hover:bg-panel-hover border border-border-subtle text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <Plus className="w-3 h-3 text-accent" aria-hidden="true" />
            Add Rule
          </button>
          <button
            type="button"
            aria-label={`Remove ${groupLabel.toLowerCase()}`}
            onClick={onRemoveGroup}
            className="p-1.5 text-muted hover:text-cta hover:bg-panel rounded-lg transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
          </button>
        </div>
      </div>

      <div className="space-y-2 pl-2 border-l-2 border-accent/25">
        {group.children.map((child, idx) => {
          if (child.kind !== 'rule') return null;
          return (
            <RuleRow
              key={idx}
              rule={child}
              labelPrefix={`${groupLabel} Rule ${idx + 1}`}
              libraries={libraries}
              facets={facets}
              onUpdate={(nextRule) => {
                const nextChildren = group.children.map((c, i) =>
                  i === idx ? nextRule : c
                );
                onUpdate(buildGroupNode(group.op, negated, nextChildren));
              }}
              onRemove={() => {
                const nextChildren = group.children.filter((_, i) => i !== idx);
                if (nextChildren.length === 0) {
                  onRemoveGroup();
                } else {
                  onUpdate(buildGroupNode(group.op, negated, nextChildren));
                }
              }}
            />
          );
        })}
      </div>
    </div>
  );
};

export const RuleBuilderEditor: React.FC<RuleBuilderEditorProps> = ({
  value,
  onChange,
  libraries = [],
  facets = null,
  onValidationErrorChange,
}) => {
  const jsonEditorId = useId();
  const initialDepth = getFilterTreeDepth(value);
  const [mode, setMode] = useState<'visual' | 'json'>(
    initialDepth > VISUAL_MAX_FILTER_DEPTH ? 'json' : 'visual'
  );
  const [rootOp, setRootOp] = useState<LogicalOp>(
    value?.kind === 'group' ? value.op : 'and'
  );
  const [rootNegated, setRootNegated] = useState<boolean>(Boolean(value?.negated));
  const [jsonDraft, setJsonDraft] = useState<string>(() =>
    value ? JSON.stringify(value, null, 2) : ''
  );
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [depthNotice, setDepthNotice] = useState<string | null>(null);

  const activeValidationError = mode === 'json' ? jsonError : null;
  useEffect(() => {
    onValidationErrorChange?.(activeValidationError);
  }, [activeValidationError, onValidationErrorChange]);

  useEffect(() => {
    if (value?.kind === 'group') {
      setRootOp(value.op);
      setRootNegated(Boolean(value.negated));
    } else if (!value) {
      setRootOp('and');
      setRootNegated(false);
    }
    const depth = getFilterTreeDepth(value);
    if (depth > VISUAL_MAX_FILTER_DEPTH) {
      setMode('json');
    }
  }, [value]);

  const visualRoot = normalizeVisualRoot(value, rootOp, rootNegated);

  const emitTree = (next: FilterNode | null) => {
    setDepthNotice(null);
    setJsonError(null);
    setJsonDraft(next ? JSON.stringify(next, null, 2) : '');
    onChange(next);
  };

  const handleSelectVisualTab = () => {
    if (jsonError) {
      return;
    }
    const currentDepth = getFilterTreeDepth(value);
    if (currentDepth > VISUAL_MAX_FILTER_DEPTH) {
      setMode('json');
      setDepthNotice(
        `Visual builder supports up to ${VISUAL_MAX_FILTER_DEPTH} nesting levels (current tree depth is ${currentDepth}). Flatten nested groups in JSON AST or clear rules to return to Visual mode.`
      );
      return;
    }
    setDepthNotice(null);
    setMode('visual');
  };

  const handleSelectJsonTab = () => {
    setDepthNotice(null);
    setJsonError(null);
    setJsonDraft(value ? JSON.stringify(value, null, 2) : '');
    setMode('json');
  };

  const handleJsonChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const nextRaw = e.target.value;
    setJsonDraft(nextRaw);
    setDepthNotice(null);

    const { node, error } = parseFilterNodeJson(nextRaw);
    if (error) {
      setJsonError(error);
      return;
    }
    setJsonError(null);
    onChange(node);
  };

  const handleToggleRootOp = (nextOp: LogicalOp) => {
    setRootOp(nextOp);
    if (visualRoot) {
      emitTree(buildGroupNode(nextOp, Boolean(visualRoot.negated), visualRoot.children));
    }
  };

  const handleToggleRootNegated = () => {
    const nextNegated = !(visualRoot ? Boolean(visualRoot.negated) : rootNegated);
    setRootNegated(nextNegated);
    if (visualRoot) {
      emitTree(buildGroupNode(visualRoot.op, nextNegated, visualRoot.children));
    }
  };

  const handleAddRootRule = () => {
    const newRule = createDefaultRule();
    if (!visualRoot) {
      emitTree(buildGroupNode(rootOp, rootNegated, [newRule]));
    } else {
      emitTree(
        buildGroupNode(visualRoot.op, Boolean(visualRoot.negated), [
          ...visualRoot.children,
          newRule,
        ])
      );
    }
  };

  const handleAddRuleGroup = () => {
    const newGroup = buildGroupNode('and', false, [createDefaultRule()]);
    if (!visualRoot) {
      emitTree(buildGroupNode(rootOp, rootNegated, [newGroup]));
    } else {
      emitTree(
        buildGroupNode(visualRoot.op, Boolean(visualRoot.negated), [
          ...visualRoot.children,
          newGroup,
        ])
      );
    }
  };

  const handleClearRules = () => {
    setRootOp('and');
    setRootNegated(false);
    emitTree(null);
  };

  let rootRuleCounter = 0;
  let subGroupCounter = 0;

  return (
    <div className="bg-canvas/50 border border-border-subtle rounded-2xl p-4 space-y-4">
      {/* Header & Mode Switcher */}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <SlidersHorizontal className="w-4 h-4 text-accent" aria-hidden="true" />
          <span className="text-xs font-semibold uppercase tracking-wider text-text-main">
            Declarative Rule Builder
          </span>
        </div>

        <div
          role="tablist"
          aria-label="Rule editor mode"
          className="inline-flex rounded-xl bg-panel p-1 border border-border-subtle"
        >
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'visual'}
            onClick={handleSelectVisualTab}
            className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
              mode === 'visual'
                ? 'bg-accent text-canvas'
                : 'text-muted hover:text-text-main'
            }`}
          >
            Visual
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'json'}
            onClick={handleSelectJsonTab}
            className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
              mode === 'json'
                ? 'bg-accent text-canvas'
                : 'text-muted hover:text-text-main'
            }`}
          >
            <Code2 className="w-3.5 h-3.5" aria-hidden="true" />
            JSON AST
          </button>
        </div>
      </div>

      {/* Quick Templates Strip */}
      <div className="space-y-1.5">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted">
          <Sparkles className="w-3.5 h-3.5 text-highlight" aria-hidden="true" />
          <span>Quick Templates</span>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {QUICK_TEMPLATES.map((tpl) => (
            <button
              key={tpl.id}
              type="button"
              onClick={() => emitTree(tpl.build())}
              className="px-2.5 py-1 rounded-lg text-xs font-medium bg-panel hover:bg-panel-hover border border-border-subtle text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              {tpl.label}
            </button>
          ))}
        </div>
      </div>

      {depthNotice && (
        <div
          role="status"
          className="flex items-start gap-2 p-3 rounded-xl bg-highlight/15 border border-highlight/40 text-xs text-highlight"
        >
          <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0" aria-hidden="true" />
          <span>{depthNotice}</span>
        </div>
      )}

      {mode === 'visual' ? (
        <div className="space-y-3">
          {/* Root Group Combinator Bar */}
          <div className="flex flex-wrap items-center justify-between gap-2 bg-panel/60 border border-border-subtle rounded-xl px-3 py-2">
            <div className="flex flex-wrap items-center gap-2">
              <div className="inline-flex rounded-lg bg-canvas p-0.5 border border-border-subtle">
                <button
                  type="button"
                  aria-pressed={(visualRoot?.op ?? rootOp) === 'and'}
                  onClick={() => handleToggleRootOp('and')}
                  className={`px-2.5 py-1 rounded-md text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                    (visualRoot?.op ?? rootOp) === 'and'
                      ? 'bg-accent text-canvas'
                      : 'text-muted hover:text-text-main'
                  }`}
                >
                  Match ALL (AND)
                </button>
                <button
                  type="button"
                  aria-pressed={(visualRoot?.op ?? rootOp) === 'or'}
                  onClick={() => handleToggleRootOp('or')}
                  className={`px-2.5 py-1 rounded-md text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                    (visualRoot?.op ?? rootOp) === 'or'
                      ? 'bg-accent text-canvas'
                      : 'text-muted hover:text-text-main'
                  }`}
                >
                  Match ANY (OR)
                </button>
              </div>

              <button
                type="button"
                aria-label="NOT root group"
                aria-pressed={visualRoot ? Boolean(visualRoot.negated) : rootNegated}
                onClick={handleToggleRootNegated}
                className={`px-2.5 py-1 rounded-lg text-xs font-bold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  (visualRoot ? Boolean(visualRoot.negated) : rootNegated)
                    ? 'bg-cta text-white'
                    : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                }`}
              >
                NOT
              </button>
            </div>

            {visualRoot && (
              <button
                type="button"
                onClick={handleClearRules}
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-muted hover:text-text-main hover:bg-canvas transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              >
                <RotateCcw className="w-3 h-3" aria-hidden="true" />
                Clear Rules
              </button>
            )}
          </div>

          {/* Rule Rows & Level-2 Group Cards */}
          {!visualRoot || visualRoot.children.length === 0 ? (
            <div className="border border-dashed border-border-subtle rounded-xl p-5 text-center space-y-1">
              <p className="text-xs font-medium text-text-main">
                No filter rules configured yet.
              </p>
              <p className="text-xs text-muted">
                Pick a Quick Template above or add rules below to build a custom shelf.
              </p>
            </div>
          ) : (
            <div className="space-y-2.5">
              {visualRoot.children.map((child, idx) => {
                if (child.kind === 'rule') {
                  rootRuleCounter += 1;
                  const ruleLabel = `Rule ${rootRuleCounter}`;
                  return (
                    <RuleRow
                      key={idx}
                      rule={child}
                      labelPrefix={ruleLabel}
                      libraries={libraries}
                      facets={facets}
                      onUpdate={(nextRule) => {
                        const nextChildren = visualRoot.children.map((c, i) =>
                          i === idx ? nextRule : c
                        );
                        emitTree(
                          buildGroupNode(
                            visualRoot.op,
                            Boolean(visualRoot.negated),
                            nextChildren
                          )
                        );
                      }}
                      onRemove={() => {
                        const nextChildren = visualRoot.children.filter((_, i) => i !== idx);
                        if (nextChildren.length === 0) {
                          handleClearRules();
                        } else {
                          emitTree(
                            buildGroupNode(
                              visualRoot.op,
                              Boolean(visualRoot.negated),
                              nextChildren
                            )
                          );
                        }
                      }}
                    />
                  );
                }

                const currentGroupIndex = subGroupCounter;
                subGroupCounter += 1;
                return (
                  <GroupCard
                    key={idx}
                    group={child}
                    groupIndex={currentGroupIndex}
                    libraries={libraries}
                    facets={facets}
                    onUpdate={(nextSubGroup) => {
                      const nextChildren = visualRoot.children.map((c, i) =>
                        i === idx ? nextSubGroup : c
                      );
                      emitTree(
                        buildGroupNode(
                          visualRoot.op,
                          Boolean(visualRoot.negated),
                          nextChildren
                        )
                      );
                    }}
                    onRemoveGroup={() => {
                      const nextChildren = visualRoot.children.filter((_, i) => i !== idx);
                      if (nextChildren.length === 0) {
                        handleClearRules();
                      } else {
                        emitTree(
                          buildGroupNode(
                            visualRoot.op,
                            Boolean(visualRoot.negated),
                            nextChildren
                          )
                        );
                      }
                    }}
                  />
                );
              })}
            </div>
          )}

          {/* Action Footer */}
          <div className="flex flex-wrap items-center gap-2 pt-1">
            <button
              type="button"
              onClick={handleAddRootRule}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-panel hover:bg-panel-hover border border-border-subtle text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              + Add Rule
            </button>
            <button
              type="button"
              onClick={handleAddRuleGroup}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-panel hover:bg-panel-hover border border-border-subtle text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <FolderPlus className="w-3.5 h-3.5 text-accent" aria-hidden="true" />
              + Add Rule Group
            </button>
          </div>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <label
              htmlFor={jsonEditorId}
              className="block text-xs font-semibold uppercase tracking-wider text-muted"
            >
              Filter JSON AST
            </label>
            {value && (
              <button
                type="button"
                onClick={handleClearRules}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium text-muted hover:text-text-main hover:bg-panel transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              >
                <RotateCcw className="w-3 h-3" aria-hidden="true" />
                Clear Rules
              </button>
            )}
          </div>
          <textarea
            id={jsonEditorId}
            rows={10}
            spellCheck={false}
            autoCapitalize="none"
            autoCorrect="off"
            aria-invalid={jsonError ? 'true' : undefined}
            value={jsonDraft}
            onChange={handleJsonChange}
            placeholder={'{\n  "kind": "group",\n  "op": "and",\n  "children": []\n}'}
            className="w-full bg-canvas border border-border-subtle rounded-xl p-3 font-mono text-xs text-text-main placeholder:text-muted/60 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          />
          {jsonError && (
            <div
              role="alert"
              className="flex items-start gap-2 p-2.5 rounded-xl bg-cta/15 border border-cta/40 text-xs text-text-main"
            >
              <AlertTriangle className="w-4 h-4 text-cta mt-0.5 shrink-0" aria-hidden="true" />
              <span>{jsonError}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
