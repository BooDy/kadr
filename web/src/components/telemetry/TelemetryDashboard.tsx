import { useState, useEffect, useCallback, type FC } from 'react';
import {
  Activity,
  Cpu,
  Database,
  Radio,
  RefreshCw,
  Users,
  Clock,
  Layers,
  Film,
  Subtitles,
  Loader2,
} from 'lucide-react';
import { api } from '../../api/client';
import type { TelemetrySnapshot, SystemEvent } from '../../types';

export interface FeedEventItem {
  id: string;
  receivedAt: number;
  event: SystemEvent;
}

export function formatMemoryMb(bytes: number): string {
  if (!isFinite(bytes) || bytes < 0) return '0.0 MB';
  const mb = bytes / (1024 * 1024);
  return `${mb.toFixed(1)} MB`;
}

export function formatBytes(bytes: number): string {
  if (!isFinite(bytes) || bytes < 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatTimestamp(ts: number): string {
  if (!ts) return 'Never';
  // Check if timestamp is in seconds or milliseconds
  const ms = ts < 1e11 ? ts * 1000 : ts;
  return new Date(ms).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

export const TelemetryDashboard: FC = () => {
  const [telemetry, setTelemetry] = useState<TelemetrySnapshot | null>(null);
  const [events, setEvents] = useState<FeedEventItem[]>([]);
  const [connectionStatus, setConnectionStatus] = useState<'Connected' | 'Reconnecting'>('Reconnecting');
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefreshedAt, setLastRefreshedAt] = useState<number | null>(null);

  // Fetch telemetry via REST
  const fetchTelemetry = useCallback(async () => {
    setIsRefreshing(true);
    try {
      const data = await api.getTelemetry();
      setTelemetry(data);
      setLastRefreshedAt(Date.now());
    } catch {
      // Telemetry fetch error
    } finally {
      setIsRefreshing(false);
    }
  }, []);

  // Initial fetch on mount
  useEffect(() => {
    fetchTelemetry();
  }, [fetchTelemetry]);

  // Connect to SSE stream /api/v1/events
  useEffect(() => {
    if (typeof EventSource === 'undefined') {
      return;
    }
    const es = new EventSource('/api/v1/events');

    es.onopen = () => {
      setConnectionStatus('Connected');
    };

    es.onerror = () => {
      setConnectionStatus('Reconnecting');
    };

    const handleMessage = (e: MessageEvent) => {
      try {
        const raw = JSON.parse(e.data);
        let parsedEvent: SystemEvent;

        if (raw && typeof raw === 'object' && 'type' in raw && 'payload' in raw) {
          parsedEvent = raw as SystemEvent;
        } else {
          parsedEvent = {
            type: (e.type as SystemEvent['type']) || 'system:telemetry',
            payload: raw,
          } as SystemEvent;
        }

        const newItem: FeedEventItem = {
          id: `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`,
          receivedAt: Date.now(),
          event: parsedEvent,
        };

        setEvents((prev) => [newItem, ...prev.slice(0, 99)]);

        // If system:telemetry event received, update live metric cards
        if (parsedEvent.type === 'system:telemetry') {
          const snapshot = parsedEvent.payload as TelemetrySnapshot;
          if (snapshot && typeof snapshot.rss_memory_bytes === 'number') {
            setTelemetry(snapshot);
            setLastRefreshedAt(Date.now());
          }
        }
      } catch {
        // Ignore unparseable frames
      }
    };

    const eventNames = [
      'library:updated',
      'layout:changed',
      'subtitle:downloaded',
      'session:synced',
      'system:telemetry',
    ];

    eventNames.forEach((name) => {
      es.addEventListener(name, handleMessage as EventListener);
    });
    es.onmessage = handleMessage;

    return () => {
      eventNames.forEach((name) => {
        es.removeEventListener(name, handleMessage as EventListener);
      });
      es.close();
    };
  }, []);

  // Compute memory budget progress bar (budget: 30 MB)
  const rssBytes = telemetry?.rss_memory_bytes || 0;
  const rssMb = rssBytes / (1024 * 1024);
  const budgetMb = 30;
  const budgetPercent = Math.min(100, Math.max(0, Math.round((rssMb / budgetMb) * 100)));
  const isOverBudget = rssMb > budgetMb;

  const renderEventPill = (type: string) => {
    switch (type) {
      case 'library:updated':
        return (
          <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-accent/15 text-accent border border-accent/30">
            <Film className="w-3 h-3" />
            library:updated
          </span>
        );
      case 'session:synced':
        return (
          <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-highlight/15 text-highlight border border-highlight/30">
            <Radio className="w-3 h-3" />
            session:synced
          </span>
        );
      case 'subtitle:downloaded':
        return (
          <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-highlight/15 text-highlight border border-highlight/30">
            <Subtitles className="w-3 h-3" />
            subtitle:downloaded
          </span>
        );
      case 'system:telemetry':
        return (
          <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-emerald-500/15 text-emerald-400 border border-emerald-500/30">
            <Activity className="w-3 h-3" />
            system:telemetry
          </span>
        );
      case 'layout:changed':
        return (
          <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-accent/15 text-accent border border-accent/30">
            <Layers className="w-3 h-3" />
            layout:changed
          </span>
        );
      default:
        return (
          <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-panel text-muted border border-border-subtle">
            {type}
          </span>
        );
    }
  };

  const renderEventPayload = (event: SystemEvent) => {
    switch (event.type) {
      case 'library:updated':
        return (
          <span className="text-muted text-xs">
            Library <strong className="text-text-main font-mono">{event.payload.library_id}</strong> updated ({event.payload.item_count} items)
          </span>
        );
      case 'session:synced':
        return (
          <span className="text-muted text-xs">
            Session <strong className="text-text-main font-mono">{event.payload.session_id}</strong> &bull; User <span className="text-text-main">{event.payload.user_id}</span> &bull; Item #{event.payload.item_id} @ {event.payload.position_seconds}s
          </span>
        );
      case 'subtitle:downloaded':
        return (
          <span className="text-muted text-xs">
            Item #{event.payload.item_id} &bull; Lang: <strong className="text-text-main uppercase font-mono">{event.payload.language}</strong> (Track #{event.payload.subtitle_id})
          </span>
        );
      case 'system:telemetry':
        return (
          <span className="text-muted text-xs">
            RSS: <strong className="text-text-main font-mono">{formatMemoryMb(event.payload.rss_memory_bytes)}</strong> &bull; Sessions: {event.payload.active_sessions_count} &bull; DB: {formatBytes(event.payload.db_size_bytes)}
          </span>
        );
      case 'layout:changed':
        return (
          <span className="text-muted text-xs">
            Screen <strong className="text-text-main font-mono">{event.payload.screen_id}</strong> layout changed
          </span>
        );
      default:
        return (
          <span className="text-muted font-mono text-xs truncate">
            {JSON.stringify((event as { payload?: unknown }).payload)}
          </span>
        );
    }
  };

  return (
    <div className="space-y-8 pb-16">
      {/* Header & Status Bar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-border-subtle pb-6">
        <div>
          <h2 className="text-2xl font-bold tracking-tight text-text-main flex items-center gap-2.5">
            <Activity className="h-6 w-6 text-accent" />
            System Telemetry
          </h2>
          <p className="text-sm text-muted mt-1">
            Real-time memory profiling, SQLite WAL metrics, and live SSE event bus feed.
          </p>
        </div>

        <div className="flex items-center gap-3">
          {/* Connection Status Badge */}
          <div
            className={`flex items-center gap-2 px-3 py-1.5 rounded-full border text-xs font-semibold backdrop-blur-md ${
              connectionStatus === 'Connected'
                ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                : 'bg-highlight/10 text-highlight border-highlight/30'
            }`}
          >
            <span
              className={`h-2 w-2 rounded-full ${
                connectionStatus === 'Connected'
                  ? 'bg-emerald-500 animate-pulse'
                  : 'bg-highlight'
              }`}
            />
            <span>{connectionStatus}</span>
          </div>

          {/* Refresh Metrics Button */}
          <button
            onClick={() => fetchTelemetry()}
            disabled={isRefreshing}
            className="flex items-center gap-2 px-3.5 py-1.5 rounded-xl bg-panel hover:bg-panel-hover text-text-main border border-border-subtle text-xs font-semibold shadow-sm transition-all cursor-pointer disabled:opacity-50 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin text-accent' : ''}`} />
            <span>Refresh Metrics</span>
          </button>
        </div>
      </div>

      {/* Live Metric Cards Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Metric 1: Process RSS Memory with Budget indicator */}
        <div className="bg-panel border border-border-subtle rounded-xl p-5 space-y-3 relative overflow-hidden shadow-sm">
          <div className="flex items-center justify-between text-muted">
            <span className="text-xs font-semibold uppercase tracking-wider flex items-center gap-1.5 text-text-main">
              <Cpu className="w-4 h-4 text-accent" />
              Process RSS Memory
            </span>
            <span className="text-[11px] font-mono text-muted">Budget: 30 MB</span>
          </div>

          <div className="space-y-2">
            <div className="font-mono text-2xl font-bold text-text-main tracking-tight">
              {formatMemoryMb(rssBytes)}
            </div>

            {/* Budget Progress Bar */}
            <div className="space-y-1">
              <div className="h-2 w-full rounded-full bg-canvas border border-border-subtle overflow-hidden">
                <div
                  className={`h-full transition-all duration-500 rounded-full ${
                    isOverBudget
                      ? 'bg-cta'
                      : budgetPercent > 75
                      ? 'bg-highlight'
                      : 'bg-accent'
                  }`}
                  style={{ width: `${budgetPercent}%` }}
                />
              </div>
              <div className="flex justify-between text-[10px] text-muted font-mono">
                <span>{budgetPercent}% utilized</span>
                <span>≤ 30 MB budget</span>
              </div>
            </div>
          </div>
        </div>

        {/* Metric 2: SQLite DB Size */}
        <div className="bg-panel border border-border-subtle rounded-xl p-5 space-y-3 shadow-sm">
          <div className="flex items-center justify-between text-muted">
            <span className="text-xs font-semibold uppercase tracking-wider flex items-center gap-1.5 text-text-main">
              <Database className="w-4 h-4 text-accent" />
              SQLite DB Size
            </span>
          </div>
          <div className="space-y-1">
            <div className="font-mono text-2xl font-bold text-text-main tracking-tight">
              {formatBytes(telemetry?.db_size_bytes ?? 0)}
            </div>
            <p className="text-[11px] text-muted">Primary library SQLite store</p>
          </div>
        </div>

        {/* Metric 3: SQLite WAL File Size */}
        <div className="bg-panel border border-border-subtle rounded-xl p-5 space-y-3 shadow-sm">
          <div className="flex items-center justify-between text-muted">
            <span className="text-xs font-semibold uppercase tracking-wider flex items-center gap-1.5 text-text-main">
              <Database className="w-4 h-4 text-highlight" />
              SQLite WAL Size
            </span>
          </div>
          <div className="space-y-1">
            <div className="font-mono text-2xl font-bold text-text-main tracking-tight">
              {formatBytes(telemetry?.wal_size_bytes ?? 0)}
            </div>
            <p className="text-[11px] text-muted">Write-Ahead Log active buffer</p>
          </div>
        </div>

        {/* Metric 4: Active Playback Sessions & Timestamp */}
        <div className="bg-panel border border-border-subtle rounded-xl p-5 space-y-3 shadow-sm">
          <div className="flex items-center justify-between text-muted">
            <span className="text-xs font-semibold uppercase tracking-wider flex items-center gap-1.5 text-text-main">
              <Users className="w-4 h-4 text-accent" />
              Active Sessions
            </span>
            <span className="flex items-center gap-1 text-[11px] font-mono text-muted">
              <Clock className="w-3 h-3" />
              {lastRefreshedAt ? formatTimestamp(lastRefreshedAt) : formatTimestamp(telemetry?.timestamp ?? 0)}
            </span>
          </div>
          <div className="space-y-1">
            <div className="font-mono text-2xl font-bold text-text-main tracking-tight">
              {telemetry?.active_sessions_count ?? 0}
            </div>
            <p className="text-[11px] text-muted">Concurrent client streams</p>
          </div>
        </div>
      </div>

      {/* Live Event Feed (/api/v1/events) */}
      <div className="bg-panel border border-border-subtle rounded-xl p-6 space-y-4 shadow-sm">
        <div className="flex items-center justify-between border-b border-border-subtle pb-4">
          <div className="flex items-center gap-3">
            <div className="h-8 w-8 rounded-lg bg-accent/15 border border-accent/30 flex items-center justify-center text-accent">
              <Radio className="w-4 h-4" />
            </div>
            <div>
              <h3 className="text-base font-bold text-text-main tracking-tight">
                Live SSE Event Feed
              </h3>
              <p className="text-xs text-muted">
                Streaming from <code className="text-accent font-mono">/api/v1/events</code>
              </p>
            </div>
          </div>
          <span className="text-xs font-mono text-muted bg-canvas px-2.5 py-1 rounded-lg border border-border-subtle">
            {events.length} captured
          </span>
        </div>

        {events.length === 0 ? (
          <div className="py-12 flex flex-col items-center justify-center text-muted space-y-2">
            <Loader2 className="w-6 h-6 animate-spin text-accent" />
            <p className="text-xs">Listening for broadcast events...</p>
          </div>
        ) : (
          <div className="space-y-2.5 max-h-[460px] overflow-y-auto pr-1">
            {events.map((item) => (
              <div
                key={item.id}
                className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 sm:gap-4 p-3 rounded-xl bg-canvas/80 border border-border-subtle hover:bg-panel-hover transition-colors"
              >
                <div className="flex items-center gap-3 flex-wrap">
                  {renderEventPill(item.event.type)}
                  {renderEventPayload(item.event)}
                </div>
                <span className="font-mono text-xs text-muted self-end sm:self-auto flex-shrink-0">
                  {formatTimestamp(item.receivedAt)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default TelemetryDashboard;
