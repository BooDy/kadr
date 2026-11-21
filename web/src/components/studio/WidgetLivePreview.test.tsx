import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, act } from '@testing-library/react';
import { WidgetLivePreview } from './WidgetLivePreview';
import { api, ApiClient, ApiError } from '../../api/client';
import type { WidgetQueryBinding } from '../../types';

describe('WidgetLivePreview', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  const baseBinding: WidgetQueryBinding = {
    macro_type: 'custom_query',
    limit: 20,
    filters: {
      rule_tree: {
        kind: 'group',
        op: 'and',
        children: [
          {
            kind: 'rule',
            condition: { field: 'country', op: 'eq', value: 'Egypt' },
          },
        ],
      },
    },
  };

  it('debounces api.previewWidget by 250ms and renders match count badge plus returned card titles', async () => {
    const previewSpy = vi
      .spyOn(api as unknown as { previewWidget: typeof api.previewWidget }, 'previewWidget')
      .mockResolvedValue({
        items: [
          { id: 1, title: 'Cairo Station', media_type: 'movie', release_year: 1958 },
          { id: 2, title: 'The Night of Counting the Years', media_type: 'movie', release_year: 1969 },
          { id: 3, title: 'Alexandria... Why?', media_type: 'movie', release_year: 1979 },
        ],
        total_count: 3,
        execution_ms: 2,
      });

    const { rerender } = render(<WidgetLivePreview binding={baseBinding} screenId="home" />);

    // Rapid intermediate update before 250ms elapses should reset debounce
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    expect(previewSpy).not.toHaveBeenCalled();

    const updatedBinding: WidgetQueryBinding = {
      ...baseBinding,
      sort: 'release_year:desc',
    };
    rerender(<WidgetLivePreview binding={updatedBinding} screenId="home" />);

    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    expect(previewSpy).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60);
    });

    expect(previewSpy).toHaveBeenCalledTimes(1);
    expect(previewSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        binding: updatedBinding,
        screen_id: 'home',
      }),
      expect.any(AbortSignal)
    );

    expect(screen.getByText('3 matches · 2 ms')).toBeInTheDocument();
    expect(screen.getByText('Cairo Station')).toBeInTheDocument();
    expect(screen.getByText('The Night of Counting the Years')).toBeInTheDocument();
    expect(screen.getByText('Alexandria... Why?')).toBeInTheDocument();
  });

  it('renders an inline error alert when api.previewWidget rejects with a 400 validation error', async () => {
    vi.spyOn(
      api as unknown as { previewWidget: typeof api.previewWidget },
      'previewWidget'
    ).mockRejectedValue(new ApiError(400, 'Filter group cannot have empty children'));

    render(<WidgetLivePreview binding={baseBinding} screenId="movies" />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(260);
    });

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Filter group cannot have empty children');
  });

  it('guards gracefully when api.previewWidget is undefined on legacy partial api mocks', async () => {
    const apiRecord = api as unknown as Record<string, unknown>;
    const original = apiRecord.previewWidget;
    delete apiRecord.previewWidget;

    try {
      expect(() => {
        render(<WidgetLivePreview binding={baseBinding} />);
      }).not.toThrow();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });
    } finally {
      if (original !== undefined) {
        apiRecord.previewWidget = original;
      }
    }
  });

  it('renders empty state hint when preview returns 0 items and verifies ApiClient facets/preview endpoints', async () => {
    vi.spyOn(api, 'previewWidget').mockResolvedValue({
      items: [],
      total_count: 0,
      execution_ms: 1,
    });

    render(<WidgetLivePreview binding={baseBinding} />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(260);
    });

    expect(screen.getByText('0 matches · 1 ms')).toBeInTheDocument();
    expect(screen.getByText(/0 items matched this query/i)).toBeInTheDocument();

    vi.restoreAllMocks();
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ 'content-type': 'application/json' }),
      text: async () =>
        JSON.stringify({
          genres: ['Drama'],
          countries: ['Egypt'],
          languages: [],
          studios: [],
          directors: [],
          tags: [],
          resolutions: [],
          video_codecs: [],
          audio_codecs: [],
          containers: [],
          year_min: 1958,
          year_max: 2026,
        }),
    });
    vi.stubGlobal('fetch', fetchMock);

    const client = new ApiClient('http://localhost:8096');
    await client.getCatalogFacets('lib-arab');
    expect(fetchMock).toHaveBeenLastCalledWith(
      'http://localhost:8096/api/v1/catalog/facets?library_id=lib-arab',
      expect.any(Object)
    );

    const abortController = new AbortController();
    fetchMock.mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: new Headers({ 'content-type': 'application/json' }),
      text: async () => JSON.stringify({ items: [], total_count: 0, execution_ms: 1 }),
    });
    await client.previewWidget({ binding: baseBinding, screen_id: 'home', limit: 8 }, abortController.signal);
    expect(fetchMock).toHaveBeenLastCalledWith(
      'http://localhost:8096/api/v1/widgets/preview',
      expect.objectContaining({
        method: 'POST',
        signal: abortController.signal,
      })
    );
  });
});
