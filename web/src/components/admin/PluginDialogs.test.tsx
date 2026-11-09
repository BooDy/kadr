import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ApprovePluginDialog } from './ApprovePluginDialog';
import { RemovePluginDialog } from './RemovePluginDialog';
import { ConnectionDialog } from './ConnectionDialog';
import { api, ApiError } from '../../api/client';
import type { PluginRecord } from '../../types';

const plugin: PluginRecord = {
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
  created_at: 1700000000,
};

describe('ApprovePluginDialog', () => {
  it('lists every requested permission, all checked by default', () => {
    render(<ApprovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole('dialog', { name: /Approve Hello Plugin/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/Publish items/i)).toBeChecked();
    expect(screen.getByLabelText(/Resolve playback/i)).toBeChecked();
  });

  it('confirms only the checked permissions, in manifest order', () => {
    const onConfirm = vi.fn();
    render(<ApprovePluginDialog plugin={plugin} onConfirm={onConfirm} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByLabelText(/Resolve playback/i));
    fireEvent.click(screen.getByRole('button', { name: /Approve and enable/i }));
    expect(onConfirm).toHaveBeenCalledWith(['items:publish']);
  });

  it('can approve with no permissions and can be cancelled', () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ApprovePluginDialog plugin={plugin} onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByLabelText(/Publish items/i));
    fireEvent.click(screen.getByLabelText(/Resolve playback/i));
    fireEvent.click(screen.getByRole('button', { name: /Approve and enable/i }));
    expect(onConfirm).toHaveBeenCalledWith([]);
    fireEvent.click(screen.getByRole('button', { name: /^Cancel$/i }));
    expect(onCancel).toHaveBeenCalled();
  });

  it('explains plugins that request nothing and disables the button while busy', () => {
    const none = { ...plugin, manifest: { ...plugin.manifest, permissions: [] } };
    render(<ApprovePluginDialog plugin={none} busy onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByText(/does not request any permissions/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Approve and enable/i })).toBeDisabled();
  });
});

describe('RemovePluginDialog', () => {
  it('defaults to keeping the data and confirms with purge only when checked', () => {
    const onConfirm = vi.fn();
    render(<RemovePluginDialog plugin={plugin} onConfirm={onConfirm} onCancel={vi.fn()} />);
    const purge = screen.getByLabelText(/Also delete this plugin's libraries and items/i);
    expect(purge).not.toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    expect(onConfirm).toHaveBeenLastCalledWith(false);

    fireEvent.click(purge);
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    expect(onConfirm).toHaveBeenLastCalledWith(true);
  });

  it('can be cancelled', () => {
    const onCancel = vi.fn();
    render(<RemovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole('button', { name: /^Cancel$/i }));
    expect(onCancel).toHaveBeenCalled();
  });
});

describe('ConnectionDialog', () => {
  const writeText = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    vi.spyOn(api, 'getPluginConnection').mockResolvedValue({
      base_url: 'http://127.0.0.1:9100',
      secret: 'SECRET-VALUE',
    });
    vi.spyOn(api, 'issuePluginToken').mockResolvedValue({ token: 'TOKEN-VALUE' });
  });

  it('shows the base URL and secret and copies the secret', async () => {
    render(<ConnectionDialog plugin={plugin} onClose={vi.fn()} />);
    expect(screen.getByRole('dialog', { name: /Connection details for Hello Plugin/i })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(screen.getByDisplayValue('http://127.0.0.1:9100')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Copy secret/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('SECRET-VALUE'));
  });

  it('disables token generation when the plugin is pending', async () => {
    render(<ConnectionDialog plugin={{ ...plugin, status: 'pending' }} onClose={vi.fn()} />);
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: /Generate token/i })).toBeDisabled();
  });

  it('generates a token on demand and builds the environment block', async () => {
    render(<ConnectionDialog plugin={{ ...plugin, status: 'enabled' }} onClose={vi.fn()} />);
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(api.issuePluginToken).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: /Generate token/i }));
    await waitFor(() => expect(screen.getByDisplayValue('TOKEN-VALUE')).toBeInTheDocument());
    expect(api.issuePluginToken).toHaveBeenCalledWith('hello');

    const env = screen.getByTestId('connection-env');
    expect(env.textContent).toContain('KADR_PLUGIN_SECRET=SECRET-VALUE');
    expect(env.textContent).toContain('KADR_PLUGIN_TOKEN=TOKEN-VALUE');
    expect(env.textContent).toContain(`KADR_URL=${window.location.origin}`);

    fireEvent.click(screen.getByRole('button', { name: /Copy environment variables/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(env.textContent));
  });

  it('shows API error messages and closes', async () => {
    vi.spyOn(api, 'getPluginConnection').mockRejectedValue(
      new ApiError(404, 'NOT_FOUND', { error: 'NOT_FOUND', message: "plugin 'hello' not found" })
    );
    const onClose = vi.fn();
    render(<ConnectionDialog plugin={plugin} onClose={onClose} />);
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent("plugin 'hello' not found"));
    fireEvent.click(screen.getByRole('button', { name: /^Close$/i }));
    expect(onClose).toHaveBeenCalled();
  });
});
