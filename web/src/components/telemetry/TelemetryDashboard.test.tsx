import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { TelemetryDashboard } from './TelemetryDashboard';
import { api } from '../../api/client';
import type { TelemetrySnapshot } from '../../types';

// Mock EventSource implementation for Vitest
class MockEventSource {
  public url: string;
  public onopen: ((ev: Event) => void) | null = null;
  public onerror: ((ev: Event) => void) | null = null;
  public onmessage: ((ev: MessageEvent) => void) | null = null;
  public listeners: Record<string, ((ev: MessageEvent) => void)[]> = {};
  public closed = false;

  static instances: MockEventSource[] = [];

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
    // Simulate open connection asynchronously
    setTimeout(() => {
      if (!this.closed && this.onopen) {
        this.onopen(new Event('open'));
      }
    }, 0);
  }

  addEventListener(type: string, listener: (ev: MessageEvent) => void) {
    if (!this.listeners[type]) {
      this.listeners[type] = [];
    }
    this.listeners[type].push(listener);
  }

  removeEventListener(type: string, listener: (ev: MessageEvent) => void) {
    if (this.listeners[type]) {
      this.listeners[type] = this.listeners[type].filter((l) => l !== listener);
    }
  }

  close() {
    this.closed = true;
  }

  // Helper method for tests to simulate incoming event
  emitEvent(eventType: string, data: unknown) {
    const event = new MessageEvent(eventType, {
      data: typeof data === 'string' ? data : JSON.stringify(data),
    });
    if (this.listeners[eventType] && this.listeners[eventType].length > 0) {
      this.listeners[eventType].forEach((listener) => listener(event));
    } else if (this.onmessage) {
      this.onmessage(event);
    }
  }

  // Helper method for tests to simulate connection error
  emitError() {
    if (this.onerror) {
      this.onerror(new Event('error'));
    }
  }
}

describe('TelemetryDashboard Component', () => {
  const originalEventSource = globalThis.EventSource;

  const initialTelemetry: TelemetrySnapshot = {
    active_sessions_count: 3,
    rss_memory_bytes: 19_293_798, // ~18.4 MB
    db_size_bytes: 262_144, // 256.0 KB
    wal_size_bytes: 65_536, // 64.0 KB
    timestamp: 1_700_000_000,
  };

  beforeEach(() => {
    vi.restoreAllMocks();
    MockEventSource.instances = [];
    (globalThis as unknown as { EventSource: unknown }).EventSource = MockEventSource;
    vi.spyOn(api, 'getTelemetry').mockResolvedValue(initialTelemetry);
  });

  afterEach(() => {
    (globalThis as unknown as { EventSource: unknown }).EventSource = originalEventSource;
  });

  it('renders telemetry dashboard header and metric cards', async () => {
    render(<TelemetryDashboard />);

    expect(screen.getByText('System Telemetry')).toBeDefined();

    await waitFor(() => {
      expect(api.getTelemetry).toHaveBeenCalled();
    });

    // Check RSS memory card formatted and budget indicator
    expect(screen.getByText(/18\.4\s*MB/i)).toBeDefined();
    expect(screen.getAllByText(/30\s*MB/i).length).toBeGreaterThan(0);

    // Check DB Size
    expect(screen.getByText(/256(\.0)?\s*KB/i)).toBeDefined();

    // Check WAL Size
    expect(screen.getByText(/64(\.0)?\s*KB/i)).toBeDefined();

    // Check Active Sessions
    expect(screen.getByText('3')).toBeDefined();
  });

  it('allows manual refresh of telemetry metrics', async () => {
    render(<TelemetryDashboard />);

    await waitFor(() => {
      expect(screen.getByText(/18\.4\s*MB/i)).toBeDefined();
    });

    const updatedTelemetry: TelemetrySnapshot = {
      active_sessions_count: 5,
      rss_memory_bytes: 23_068_672, // 22.0 MB
      db_size_bytes: 524_288, // 512.0 KB
      wal_size_bytes: 131_072, // 128.0 KB
      timestamp: 1_700_000_100,
    };
    vi.spyOn(api, 'getTelemetry').mockResolvedValue(updatedTelemetry);

    const refreshButton = screen.getByRole('button', { name: /refresh metrics/i });
    fireEvent.click(refreshButton);

    await waitFor(() => {
      expect(screen.getByText(/22(\.0)?\s*MB/i)).toBeDefined();
      expect(screen.getByText('5')).toBeDefined();
    });
  });

  it('connects to /api/v1/events and displays connection status', async () => {
    render(<TelemetryDashboard />);

    expect(MockEventSource.instances.length).toBe(1);
    expect(MockEventSource.instances[0].url).toBe('/api/v1/events');

    await waitFor(() => {
      expect(screen.getByText(/connected/i)).toBeDefined();
    });

    // Simulate error to trigger reconnecting badge
    act(() => {
      MockEventSource.instances[0].emitError();
    });

    await waitFor(() => {
      expect(screen.getByText(/reconnecting/i)).toBeDefined();
    });
  });

  it('receives and renders SSE events in the live event feed with color-coded pills', async () => {
    render(<TelemetryDashboard />);

    await waitFor(() => {
      expect(screen.getByText(/connected/i)).toBeDefined();
    });

    const eventSource = MockEventSource.instances[0];

    // Emit library:updated event
    act(() => {
      eventSource.emitEvent('library:updated', {
        type: 'library:updated',
        payload: {
          library_id: 'lib-movies-1',
          item_count: 42,
          timestamp: 1700000010,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByText(/library:updated/i)).toBeDefined();
      expect(screen.getByText(/lib-movies-1/i)).toBeDefined();
      expect(screen.getByText(/42 items/i)).toBeDefined();
    });

    // Emit session:synced event
    act(() => {
      eventSource.emitEvent('session:synced', {
        type: 'session:synced',
        payload: {
          session_id: 'sess-abc',
          item_id: 101,
          user_id: 'user-1',
          position_seconds: 350,
          timestamp: 1700000020,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByText(/session:synced/i)).toBeDefined();
      expect(screen.getByText(/sess-abc/i)).toBeDefined();
    });

    // Emit subtitle:downloaded event
    act(() => {
      eventSource.emitEvent('subtitle:downloaded', {
        type: 'subtitle:downloaded',
        payload: {
          item_id: 101,
          subtitle_id: 9,
          language: 'eng',
          timestamp: 1700000030,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByText(/subtitle:downloaded/i)).toBeDefined();
      expect(screen.getByText(/eng/i)).toBeDefined();
    });
  });

  it('updates telemetry metrics when system:telemetry SSE event is received', async () => {
    render(<TelemetryDashboard />);

    await waitFor(() => {
      expect(screen.getByText(/18\.4\s*MB/i)).toBeDefined();
    });

    const eventSource = MockEventSource.instances[0];

    // Emit system:telemetry event
    act(() => {
      eventSource.emitEvent('system:telemetry', {
        type: 'system:telemetry',
        payload: {
          active_sessions_count: 7,
          rss_memory_bytes: 26_214_400, // 25.0 MB
          db_size_bytes: 1_048_576, // 1.0 MB
          wal_size_bytes: 262_144, // 256.0 KB
          timestamp: 1700000050,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getAllByText(/25(\.0)?\s*MB/i).length).toBeGreaterThan(0);
      expect(screen.getByText('7')).toBeDefined();
      expect(screen.getByText(/system:telemetry/i)).toBeDefined();
    });
  });

  it('cleans up EventSource on unmount', async () => {
    const { unmount } = render(<TelemetryDashboard />);

    expect(MockEventSource.instances.length).toBe(1);
    const es = MockEventSource.instances[0];
    expect(es.closed).toBe(false);

    unmount();

    expect(es.closed).toBe(true);
  });
});
