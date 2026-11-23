import { useState } from 'react';
import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RuleBuilderEditor } from './RuleBuilderEditor';
import { getFilterTreeDepth, type CatalogFacets, type FilterNode } from '../../types/filterAst';
import type { Library } from '../../types';

const mockLibraries: Library[] = [
  {
    id: 'lib-movies',
    name: 'World Cinema',
    path: '/media/movies',
    media_type: 'movie',
    is_private: false,
    created_at: 1700000000,
  },
  {
    id: 'lib-shows',
    name: 'Series Vault',
    path: '/media/shows',
    media_type: 'show',
    is_private: false,
    created_at: 1700000000,
  },
];

const mockFacets: CatalogFacets = {
  genres: ['Drama', 'Sci-Fi', 'Thriller'],
  countries: ['Egypt', 'France', 'Japan'],
  languages: ['Arabic', 'English', 'Japanese'],
  studios: ['Misr International', 'Studio Ghibli'],
  directors: ['Youssef Chahine', 'Denis Villeneuve'],
  tags: ['4k-remaster', 'festival-winner'],
  resolutions: ['2160p', '1080p'],
  video_codecs: ['hevc', 'h264'],
  audio_codecs: ['eac3', 'flac'],
  containers: ['mkv', 'mp4'],
  year_min: 1958,
  year_max: 2026,
};

function ControlledHarness({
  initialValue = null,
  onEmit,
}: {
  initialValue?: FilterNode | null;
  onEmit?: (next: FilterNode | null) => void;
}) {
  const [tree, setTree] = useState<FilterNode | null>(initialValue);
  const handleChange = (next: FilterNode | null) => {
    setTree(next);
    onEmit?.(next);
  };
  return (
    <RuleBuilderEditor
      value={tree}
      onChange={handleChange}
      libraries={mockLibraries}
      facets={mockFacets}
    />
  );
}

describe('RuleBuilderEditor', () => {
  it('emits a valid FilterNode group when clicking Quick Template "Short Films (< 90 min)"', () => {
    const handleChange = vi.fn();
    render(
      <RuleBuilderEditor
        value={null}
        onChange={handleChange}
        libraries={mockLibraries}
        facets={mockFacets}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /Short Films \(< 90 min\)/i }));

    expect(handleChange).toHaveBeenCalledTimes(1);
    expect(handleChange).toHaveBeenLastCalledWith({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'item_type',
            op: 'in',
            values: ['movie'],
          },
        },
        {
          kind: 'rule',
          condition: {
            field: 'duration_seconds',
            op: 'lte',
            value: 5400,
          },
        },
      ],
    });
  });

  it('builds exact FilterNode structures when adding rules, toggling Match ANY (OR) and NOT, picking Country facet Egypt, converting 90 min -> 5400 sec, and adding a nested Rule Group', () => {
    let latestTree: FilterNode | null = null;
    render(
      <ControlledHarness
        onEmit={(next) => {
          latestTree = next;
        }}
      />
    );

    // 1. Add first rule
    fireEvent.click(screen.getByRole('button', { name: /^\+ Add Rule$/i }));
    expect(latestTree).not.toBeNull();

    // 2. Toggle root group to Match ANY (OR) and root NOT
    fireEvent.click(screen.getByRole('button', { name: /Match ANY \(OR\)/i }));
    fireEvent.click(screen.getByRole('button', { name: /NOT root group/i }));

    // 3. Select Country field and pick Egypt from facet suggestions
    const fieldSelect = screen.getByLabelText('Rule 1 field');
    fireEvent.change(fieldSelect, { target: { value: 'country' } });

    const countryInput = screen.getByLabelText('Rule 1 value') as HTMLInputElement;
    const datalistId = countryInput.getAttribute('list');
    expect(datalistId).toBeTruthy();
    expect(document.querySelector(`datalist#${datalistId} option[value="Egypt"]`)).not.toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Egypt' }));

    expect(latestTree).toEqual({
      kind: 'group',
      op: 'or',
      negated: true,
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'country',
            op: 'eq',
            value: 'Egypt',
          },
        },
      ],
    });

    // 4. Add a second rule for duration_seconds and enter 90 minutes -> 5400 seconds
    fireEvent.click(screen.getByRole('button', { name: /^\+ Add Rule$/i }));
    const secondFieldSelect = screen.getByLabelText('Rule 2 field');
    fireEvent.change(secondFieldSelect, { target: { value: 'duration_seconds' } });

    const secondOpSelect = screen.getByLabelText('Rule 2 operator');
    fireEvent.change(secondOpSelect, { target: { value: 'lte' } });

    const durationMinutesInput = screen.getByLabelText('Rule 2 value (minutes)');
    fireEvent.change(durationMinutesInput, { target: { value: '90' } });

    // 5. Add a nested Level-2 Rule Group
    fireEvent.click(screen.getByRole('button', { name: /\+ Add Rule Group/i }));

    expect(latestTree).toEqual({
      kind: 'group',
      op: 'or',
      negated: true,
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'country',
            op: 'eq',
            value: 'Egypt',
          },
        },
        {
          kind: 'rule',
          condition: {
            field: 'duration_seconds',
            op: 'lte',
            value: 5400,
          },
        },
        {
          kind: 'group',
          op: 'and',
          children: [
            {
              kind: 'rule',
              condition: {
                field: 'item_type',
                op: 'in',
                values: ['movie'],
              },
            },
          ],
        },
      ],
    });
    expect(getFilterTreeDepth(latestTree!)).toBe(2);
  });

  it('converts watch_state progress_between percentages (0-100%) to ratios (0.0-1.0) and clears rules', () => {
    let latestTree: FilterNode | null = null;
    render(
      <ControlledHarness
        onEmit={(next) => {
          latestTree = next;
        }}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /^\+ Add Rule$/i }));
    fireEvent.change(screen.getByLabelText('Rule 1 field'), { target: { value: 'watch_state' } });
    fireEvent.change(screen.getByLabelText('Rule 1 operator'), {
      target: { value: 'progress_between' },
    });

    fireEvent.change(screen.getByLabelText('Rule 1 min (%)'), { target: { value: '25' } });
    fireEvent.change(screen.getByLabelText('Rule 1 max (%)'), { target: { value: '85' } });

    expect(latestTree).toEqual({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'watch_state',
            op: 'progress_between',
            min: 0.25,
            max: 0.85,
          },
        },
      ],
    });

    fireEvent.click(screen.getByRole('button', { name: /Clear Rules/i }));
    expect(latestTree).toBeNull();
  });

  it('allows editing depth=3 JSON in JSON AST mode and keeps JSON AST active with a notice banner when attempting to switch to Visual while depth > 2', () => {
    let latestTree: FilterNode | null = null;
    render(
      <ControlledHarness
        onEmit={(next) => {
          latestTree = next;
        }}
      />
    );

    fireEvent.click(screen.getByRole('tab', { name: /JSON AST/i }));

    const deepAst: FilterNode = {
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'group',
          op: 'or',
          children: [
            {
              kind: 'group',
              op: 'and',
              children: [
                {
                  kind: 'rule',
                  condition: { field: 'country', op: 'eq', value: 'Egypt' },
                },
              ],
            },
          ],
        },
      ],
    };

    const jsonEditor = screen.getByLabelText(/Filter JSON AST/i);
    fireEvent.change(jsonEditor, { target: { value: JSON.stringify(deepAst, null, 2) } });

    expect(latestTree).toEqual(deepAst);
    expect(getFilterTreeDepth(latestTree!)).toBe(3);

    // Attempt to switch back to Visual mode when depth > 2
    fireEvent.click(screen.getByRole('tab', { name: /^Visual$/i }));

    // JSON AST stays active and notice banner explains why
    expect(screen.getByRole('tab', { name: /JSON AST/i })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText(/Filter JSON AST/i)).toBeInTheDocument();
    expect(
      screen.getByText(/Visual builder supports up to 2 nesting levels/i)
    ).toBeInTheDocument();
  });

  it('supports multi-chip array fields (Genres) via Enter key and facet chip toggles, and flags malformed JSON AST', () => {
    let latestTree: FilterNode | null = null;
    const onEmit = vi.fn((next: FilterNode | null) => {
      latestTree = next;
    });
    render(<ControlledHarness onEmit={onEmit} />);

    fireEvent.click(screen.getByRole('button', { name: /^\+ Add Rule$/i }));
    fireEvent.change(screen.getByLabelText('Rule 1 field'), { target: { value: 'genres' } });

    const chipInput = screen.getByLabelText('Rule 1 value');
    fireEvent.change(chipInput, { target: { value: 'Mystery' } });
    fireEvent.keyDown(chipInput, { key: 'Enter' });

    fireEvent.click(screen.getByRole('button', { name: 'Sci-Fi' }));

    expect(latestTree).toEqual({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'genres',
            op: 'contains_any',
            values: ['Drama', 'Mystery', 'Sci-Fi'],
          },
        },
      ],
    });

    const callCountBeforeBadJson = onEmit.mock.calls.length;
    fireEvent.click(screen.getByRole('tab', { name: /JSON AST/i }));
    fireEvent.change(screen.getByLabelText(/Filter JSON AST/i), {
      target: { value: '{ broken json' },
    });

    expect(screen.getByRole('alert')).toHaveTextContent(/Invalid JSON syntax/i);
    expect(onEmit).toHaveBeenCalledTimes(callCountBeforeBadJson);
  });

  it('accepts "unknown" item_type in JSON AST and emits onValidationErrorChange updates', () => {
    const onChange = vi.fn();
    const onValidationErrorChange = vi.fn();
    render(
      <RuleBuilderEditor
        value={null}
        onChange={onChange}
        libraries={mockLibraries}
        facets={mockFacets}
        onValidationErrorChange={onValidationErrorChange}
      />
    );

    expect(onValidationErrorChange).toHaveBeenLastCalledWith(null);

    fireEvent.click(screen.getByRole('tab', { name: /JSON AST/i }));
    const jsonEditor = screen.getByLabelText(/Filter JSON AST/i);

    fireEvent.change(jsonEditor, { target: { value: '{ broken' } });
    expect(onValidationErrorChange).toHaveBeenLastCalledWith(
      expect.stringMatching(/Invalid JSON syntax/i)
    );

    const unknownTypeAst: FilterNode = {
      kind: 'rule',
      condition: { field: 'item_type', op: 'in', values: ['unknown'] },
    };
    fireEvent.change(jsonEditor, {
      target: { value: JSON.stringify(unknownTypeAst, null, 2) },
    });

    expect(onValidationErrorChange).toHaveBeenLastCalledWith(null);
    expect(onChange).toHaveBeenLastCalledWith(unknownTypeAst);
  });

  it('supports Is Favorite boolean rule toggle and the "Unwatched Favorites" quick template', () => {
    let latestTree: FilterNode | null = null;
    render(
      <ControlledHarness
        onEmit={(next) => {
          latestTree = next;
        }}
      />
    );

    // 1. Add rule and select Is Favorite field -> defaults to { field: 'is_favorite', op: 'eq', value: true }
    fireEvent.click(screen.getByRole('button', { name: /^\+ Add Rule$/i }));
    fireEvent.change(screen.getByLabelText('Rule 1 field'), { target: { value: 'is_favorite' } });

    expect(latestTree).toEqual({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'is_favorite',
            op: 'eq',
            value: true,
          },
        },
      ],
    });

    const yesBtn = screen.getByRole('button', { name: /Yes — Favorited/i });
    const noBtn = screen.getByRole('button', { name: /No — Not Favorited/i });
    expect(yesBtn).toHaveAttribute('aria-pressed', 'true');
    expect(noBtn).toHaveAttribute('aria-pressed', 'false');

    // 2. Toggle to false -> { field: 'is_favorite', op: 'eq', value: false }
    fireEvent.click(noBtn);
    expect(noBtn).toHaveAttribute('aria-pressed', 'true');
    expect(yesBtn).toHaveAttribute('aria-pressed', 'false');
    expect(latestTree).toEqual({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'is_favorite',
            op: 'eq',
            value: false,
          },
        },
      ],
    });

    // 3. Apply "Unwatched Favorites" Quick Template
    fireEvent.click(screen.getByRole('button', { name: /Unwatched Favorites/i }));
    expect(latestTree).toEqual({
      kind: 'group',
      op: 'and',
      children: [
        {
          kind: 'rule',
          condition: {
            field: 'is_favorite',
            op: 'eq',
            value: true,
          },
        },
        {
          kind: 'rule',
          condition: {
            field: 'watch_state',
            op: 'state_in',
            values: ['unwatched'],
          },
        },
      ],
    });
  });
});
