import React, { useEffect, useState } from 'react';
import { AlertCircle, Eye, Film, Loader2, Star } from 'lucide-react';
import { api } from '../../api/client';
import type { WidgetPreviewResponse, WidgetQueryBinding } from '../../types';

export interface WidgetLivePreviewProps {
  binding: WidgetQueryBinding;
  screenId?: string;
  limit?: number;
}

function extractPreviewErrorMessage(err: unknown): string {
  if (err instanceof Error && err.message) {
    return err.message;
  }
  if (err && typeof err === 'object') {
    if ('error' in err && typeof (err as { error: unknown }).error === 'string') {
      return (err as { error: string }).error;
    }
    if ('message' in err && typeof (err as { message: unknown }).message === 'string') {
      return (err as { message: string }).message;
    }
  }
  return 'Failed to preview widget query';
}

export const WidgetLivePreview: React.FC<WidgetLivePreviewProps> = ({
  binding,
  screenId,
  limit = 10,
}) => {
  const [result, setResult] = useState<WidgetPreviewResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);

  const bindingKey = JSON.stringify(binding);

  useEffect(() => {
    if (typeof api.previewWidget !== 'function') {
      return;
    }

    const controller = new AbortController();
    setIsLoading(true);

    const timer = setTimeout(() => {
      const parsedBinding = JSON.parse(bindingKey) as WidgetQueryBinding;
      api
        .previewWidget(
          {
            binding: parsedBinding,
            ...(screenId ? { screen_id: screenId } : {}),
            limit,
          },
          controller.signal
        )
        .then((res) => {
          if (controller.signal.aborted) return;
          setResult(res);
          setError(null);
          setIsLoading(false);
        })
        .catch((err: unknown) => {
          if (controller.signal.aborted) return;
          if (err instanceof Error && err.name === 'AbortError') return;
          setError(extractPreviewErrorMessage(err));
          setIsLoading(false);
        });
    }, 250);

    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [bindingKey, screenId, limit]);

  return (
    <div className="bg-canvas/60 border border-border-subtle rounded-2xl p-4 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Eye className="w-4 h-4 text-accent" aria-hidden="true" />
          <span className="text-xs font-semibold uppercase tracking-wider text-text-main">
            Live Preview
          </span>
        </div>

        <div className="flex items-center gap-2">
          {isLoading && (
            <Loader2
              className="w-3.5 h-3.5 text-accent animate-spin"
              aria-hidden="true"
            />
          )}
          {result && !error && (
            <span
              role="status"
              className="px-2.5 py-0.5 rounded-lg bg-highlight/15 border border-highlight/30 text-highlight font-mono text-xs font-semibold"
            >
              {`${result.total_count} matches · ${result.execution_ms} ms`}
            </span>
          )}
        </div>
      </div>

      {error ? (
        <div
          role="alert"
          className="flex items-start gap-2 p-3 rounded-xl bg-cta/15 border border-cta/40 text-xs text-text-main"
        >
          <AlertCircle className="w-4 h-4 text-cta mt-0.5 shrink-0" aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : result && result.items.length === 0 ? (
        <div className="border border-dashed border-border-subtle rounded-xl p-4 text-center">
          <p className="text-xs text-muted">
            0 items matched this query. Broaden your filter conditions or switch combinators from
            AND to OR.
          </p>
        </div>
      ) : result && result.items.length > 0 ? (
        <div className="flex gap-3 overflow-x-auto pb-1">
          {result.items.map((item) => (
            <div
              key={item.id}
              className="w-24 shrink-0 bg-panel border border-border-subtle rounded-xl overflow-hidden flex flex-col"
            >
              <div className="aspect-[2/3] bg-canvas flex items-center justify-center overflow-hidden relative">
                {item.poster_url ? (
                  <img
                    src={item.poster_url}
                    alt={item.title}
                    loading="lazy"
                    className="w-full h-full object-cover"
                  />
                ) : (
                  <Film className="w-6 h-6 text-muted" aria-hidden="true" />
                )}
                {item.rating !== undefined && (
                  <span className="absolute top-1 right-1 inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-canvas/85 text-[10px] font-semibold text-highlight">
                    <Star className="w-2.5 h-2.5 fill-current" aria-hidden="true" />
                    {item.rating.toFixed(1)}
                  </span>
                )}
              </div>
              <div className="p-2">
                <p className="text-xs font-medium text-text-main truncate">{item.title}</p>
                {item.release_year !== undefined && (
                  <p className="text-[10px] text-muted mt-0.5">{item.release_year}</p>
                )}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted">
          Evaluating live catalog preview...
        </p>
      )}
    </div>
  );
};
