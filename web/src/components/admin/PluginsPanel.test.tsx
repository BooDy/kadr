import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, within, act } from '@testing-library/react';
import { PluginsPanel } from './PluginsPanel';
import { api, ApiError } from '../../api/client';
import type { PluginRecord, PluginSettings } from '../../types';

const disabledSettings: PluginSettings = {
  enabled: false,
  resolve_timeout_secs: 5,
  allow_private_proxy: false,
};
const enabledSettings: PluginSettings = { ...disabledSettings, enabled: true };

const pendingPlugin: PluginRecord = {
  id: 'hello',
  name: 'Hello Plugin',
  base_url: 'http://127.0.0.1:9100',
  manifest: {
    id: 'hello',
    name: 'Hello Plugin',
    version: '0.1.0',
    api_version: 1,
    permissions: ['items:publish', 'playback:resolve'],
    capabilities: { virtual_library: true, playback_resolver: true },
  },
  granted_permissions: [],
  status: 'pending',
  created_at: 1,
};
const enabledPlugin: PluginRecord = {
  ...pendingPlugin,
  status: 'enabled',
  granted_permissions: ['items:publish', 'playback:resolve'],
};
const disabledPlugin: PluginRecord = { ...enabledPlugin, status: 'disabled' };

function createDeferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('PluginsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(api, 'getPluginSettings').mockResolvedValue(disabledSettings);
    vi.spyOn(api, 'updatePluginSettings').mockImplementation(async (payload) => ({
      ...disabledSettings,
      ...payload,
    }));
    vi.spyOn(api, 'listPlugins').mockResolvedValue([]);
    vi.spyOn(api, 'registerPlugin').mockResolvedValue({ plugin: pendingPlugin, secret: 's' });
    vi.spyOn(api, 'approvePlugin').mockResolvedValue({ plugin: enabledPlugin, token: 't' });
    vi.spyOn(api, 'setPluginEnabled').mockResolvedValue(enabledPlugin);
    vi.spyOn(api, 'removePlugin').mockResolvedValue();
    vi.spyOn(api, 'getPluginConnection').mockResolvedValue({
      base_url: pendingPlugin.base_url,
      secret: 'SECRET-VALUE',
    });
    vi.spyOn(api, 'issuePluginToken').mockResolvedValue({ token: 'TOKEN-VALUE' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const enableSwitch = () => screen.getByRole('switch', { name: /Enable plugins/i });

  it('starts disabled: shows the notice and does not load plugins', async () => {
    render(<PluginsPanel />);
    await waitFor(() => expect(api.getPluginSettings).toHaveBeenCalled());
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));
    expect(screen.getByText(/Plugins are disabled/i)).toBeInTheDocument();
    expect(api.listPlugins).not.toHaveBeenCalled();
    expect(screen.getByLabelText(/Plugin base URL/i)).toBeDisabled();
  });

  it('enables plugins from the switch and loads the list', async () => {
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));

    fireEvent.click(enableSwitch());
    await waitFor(() => expect(api.updatePluginSettings).toHaveBeenCalledWith({ enabled: true }));
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'true'));
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(screen.queryByText(/Plugins are disabled/i)).toBeNull();
    expect(screen.getByRole('status')).toHaveTextContent(/Plugins enabled/i);
  });

  it('can disable again and clears the list', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(enableSwitch());
    await waitFor(() => expect(api.updatePluginSettings).toHaveBeenCalledWith({ enabled: false }));
    await waitFor(() => expect(screen.queryByTestId('plugin-row-hello')).toBeNull());
    expect(screen.getByText(/Plugins are disabled/i)).toBeInTheDocument();
  });

  it('loads the plugin list on mount when already enabled', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(api.listPlugins).toHaveBeenCalledTimes(1);
  });

  it('saves settings and validates the timeout client side', async () => {
    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));

    const timeout = screen.getByLabelText(/Resolve timeout/i);
    fireEvent.change(timeout, { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: /Save plugin settings/i }));
    expect(screen.getByRole('alert')).toHaveTextContent(/between 1 and 60/i);
    expect(api.updatePluginSettings).not.toHaveBeenCalled();

    fireEvent.change(timeout, { target: { value: '12' } });
    fireEvent.click(screen.getByLabelText(/Allow plugins to stream from private/i));
    fireEvent.click(screen.getByRole('button', { name: /Save plugin settings/i }));
    await waitFor(() =>
      expect(api.updatePluginSettings).toHaveBeenCalledWith({
        resolve_timeout_secs: 12,
        allow_private_proxy: true,
      })
    );
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent(/settings saved/i));
  });

  it('registers a plugin and refreshes the list', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValueOnce([]).mockResolvedValue([pendingPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByLabelText(/Plugin base URL/i)).toBeEnabled());

    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), {
      target: { value: 'http://127.0.0.1:9100' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(api.registerPlugin).toHaveBeenCalledWith('http://127.0.0.1:9100'));
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(screen.getByRole('status')).toHaveTextContent(/Registered "Hello Plugin"/);
  });

  it('shows the API message when registration fails', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.registerPlugin).mockRejectedValue(
      new ApiError(503, 'PLUGIN_UNAVAILABLE', {
        error: 'PLUGIN_UNAVAILABLE',
        message: 'Plugin unavailable: connection failed',
      })
    );
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByLabelText(/Plugin base URL/i)).toBeEnabled());
    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), { target: { value: 'http://bad' } });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Plugin unavailable: connection failed')
    );
  });

  it('approves with a permission subset', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([pendingPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Review & approve/i }));
    const dialog = screen.getByRole('dialog', { name: /Approve Hello Plugin/i });
    fireEvent.click(within(dialog).getByLabelText(/Resolve playback/i));
    fireEvent.click(within(dialog).getByRole('button', { name: /Approve and enable/i }));

    await waitFor(() => expect(api.approvePlugin).toHaveBeenCalledWith('hello', ['items:publish']));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.getByRole('status')).toHaveTextContent(/approved and enabled/i);
  });

  it('keeps dialog plugin stable during 15s polling refresh without resetting checkbox selection', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([pendingPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Review & approve/i }));
    const dialog = screen.getByRole('dialog', { name: /Approve Hello Plugin/i });
    const checkbox = within(dialog).getByLabelText(/Resolve playback/i);
    expect(checkbox).toBeChecked();

    fireEvent.click(checkbox);
    expect(checkbox).not.toBeChecked();

    // Next polling tick returns fresh object with new permissions array reference
    vi.mocked(api.listPlugins).mockResolvedValue([
      {
        ...pendingPlugin,
        manifest: {
          ...pendingPlugin.manifest,
          permissions: ['items:publish', 'playback:resolve'],
        },
      },
    ]);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(api.listPlugins).toHaveBeenCalledTimes(2);

    // Dialog state is preserved because approving plugin object in state is stable
    expect(checkbox).not.toBeChecked();
  });

  it('enables and disables a plugin', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins)
      .mockResolvedValueOnce([disabledPlugin])
      .mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /^Enable$/i }));
    await waitFor(() => expect(api.setPluginEnabled).toHaveBeenCalledWith('hello', true));

    fireEvent.click(await screen.findByRole('button', { name: /^Disable$/i }));
    await waitFor(() => expect(api.setPluginEnabled).toHaveBeenCalledWith('hello', false));
  });

  it('removes a plugin with and without purging data', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /^Remove$/i }));
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenLastCalledWith('hello', false));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

    fireEvent.click(screen.getByRole('button', { name: /^Remove$/i }));
    fireEvent.click(screen.getByLabelText(/Also delete this plugin's libraries and items/i));
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenLastCalledWith('hello', true));
  });

  it('opens the connection details dialog', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Connection details/i }));
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(api.getPluginConnection).toHaveBeenCalledWith('hello');
  });

  it('refreshes statuses every 15 seconds while enabled and keeps the last list on failure', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(api.listPlugins).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(api.listPlugins).toHaveBeenCalledTimes(2);

    vi.mocked(api.listPlugins).mockRejectedValue(new Error('network down'));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(api.listPlugins).toHaveBeenCalledTimes(3);
    expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/last known state/i));
  });

  it('does not poll while plugins are disabled', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(45000);
    });
    expect(api.listPlugins).not.toHaveBeenCalled();
  });

  it('discards stale refresh response after plugins are disabled', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValueOnce([]);

    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'true'));
    expect(screen.queryByTestId('plugin-row-hello')).not.toBeInTheDocument();

    const deferred = createDeferred<PluginRecord[]>();
    vi.mocked(api.listPlugins).mockReturnValueOnce(deferred.promise);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(api.listPlugins).toHaveBeenCalledTimes(2);

    fireEvent.click(enableSwitch());
    await waitFor(() => expect(api.updatePluginSettings).toHaveBeenCalledWith({ enabled: false }));
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));
    expect(screen.getByText(/Plugins are disabled/i)).toBeInTheDocument();

    await act(async () => {
      deferred.resolve([enabledPlugin]);
    });

    expect(screen.queryByTestId('plugin-row-hello')).not.toBeInTheDocument();
    expect(screen.getByText(/Plugins are disabled/i)).toBeInTheDocument();
  });

  it('drops out-of-order refresh responses when older call resolves after newer call', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValueOnce([pendingPlugin]);

    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    const olderRefresh = createDeferred<PluginRecord[]>();
    vi.mocked(api.listPlugins).mockReturnValueOnce(olderRefresh.promise);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(api.listPlugins).toHaveBeenCalledTimes(2);

    const newPlugin: PluginRecord = {
      ...enabledPlugin,
      id: 'newer',
      name: 'Newer Plugin',
    };
    vi.mocked(api.registerPlugin).mockResolvedValueOnce({ plugin: newPlugin, secret: 's' });
    vi.mocked(api.listPlugins).mockResolvedValueOnce([newPlugin]);

    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), {
      target: { value: 'http://127.0.0.1:9200' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    });

    await waitFor(() => expect(screen.getByTestId('plugin-row-newer')).toBeInTheDocument());

    await act(async () => {
      olderRefresh.resolve([pendingPlugin]);
    });

    expect(screen.getByTestId('plugin-row-newer')).toBeInTheDocument();
  });

  it('does not perform state updates when unmounted while listPlugins is pending', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    const deferred = createDeferred<PluginRecord[]>();
    vi.mocked(api.listPlugins).mockReturnValue(deferred.promise);

    const { unmount } = render(<PluginsPanel />);
    await waitFor(() => expect(api.getPluginSettings).toHaveBeenCalled());
    unmount();

    await act(async () => {
      deferred.resolve([enabledPlugin]);
    });

    expect(errorSpy).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it('shows approve failure inside the dialog, keeps dialog open, and clears error on retry', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([pendingPlugin]);
    vi.mocked(api.approvePlugin).mockRejectedValueOnce(
      new ApiError(400, 'INVALID_PERMISSIONS', {
        error: 'INVALID_PERMISSIONS',
        message: 'Permission rejected by security policy',
      })
    );

    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Review & approve/i }));
    const dialog = screen.getByRole('dialog', { name: /Approve Hello Plugin/i });

    fireEvent.click(within(dialog).getByRole('button', { name: /Approve and enable/i }));
    await waitFor(() => expect(api.approvePlugin).toHaveBeenCalledTimes(1));

    expect(screen.getByRole('dialog', { name: /Approve Hello Plugin/i })).toBeInTheDocument();
    expect(within(dialog).getByRole('alert')).toHaveTextContent('Permission rejected by security policy');
    expect(screen.getAllByRole('alert')).toHaveLength(1);

    vi.mocked(api.approvePlugin).mockResolvedValueOnce({ plugin: enabledPlugin, token: 't' });
    fireEvent.click(within(dialog).getByRole('button', { name: /Approve and enable/i }));

    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.getByRole('status')).toHaveTextContent(/approved and enabled/i);
  });

  it('shows remove failure inside the dialog, keeps dialog open, and clears error on retry', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    vi.mocked(api.removePlugin).mockRejectedValueOnce(
      new ApiError(409, 'PLUGIN_IN_USE', {
        error: 'PLUGIN_IN_USE',
        message: 'Cannot remove plugin: active playback session',
      })
    );

    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /^Remove$/i }));
    const dialog = screen.getByRole('dialog', { name: /Remove Hello Plugin/i });

    fireEvent.click(within(dialog).getByRole('button', { name: /Remove plugin/i }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenCalledTimes(1));

    expect(screen.getByRole('dialog', { name: /Remove Hello Plugin/i })).toBeInTheDocument();
    expect(within(dialog).getByRole('alert')).toHaveTextContent('Cannot remove plugin: active playback session');
    expect(screen.getAllByRole('alert')).toHaveLength(1);

    vi.mocked(api.removePlugin).mockResolvedValueOnce();
    fireEvent.click(within(dialog).getByRole('button', { name: /Remove plugin/i }));

    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.getByRole('status')).toHaveTextContent(/Removed "Hello Plugin"/i);
  });
});
