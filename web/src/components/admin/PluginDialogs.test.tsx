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
    // Toggle off the first permission, then toggle it back on so its selection order is reversed
    fireEvent.click(screen.getByLabelText(/Publish items/i));
    fireEvent.click(screen.getByLabelText(/Publish items/i));
    fireEvent.click(screen.getByRole('button', { name: /Approve and enable/i }));
    // Proves manifest order ['items:publish', 'playback:resolve'] is preserved
    expect(onConfirm).toHaveBeenCalledWith(['items:publish', 'playback:resolve']);

    // Also confirm unchecking one excludes it
    fireEvent.click(screen.getByLabelText(/Resolve playback/i));
    fireEvent.click(screen.getByRole('button', { name: /Approve and enable/i }));
    expect(onConfirm).toHaveBeenLastCalledWith(['items:publish']);
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

  it('closes on Escape key and backdrop click, but not on click inside dialog', () => {
    const onCancel = vi.fn();
    render(<ApprovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={onCancel} />);
    const dialog = screen.getByRole('dialog', { name: /Approve Hello Plugin/i });

    fireEvent.click(dialog);
    expect(onCancel).not.toHaveBeenCalled();

    fireEvent.click(dialog.parentElement!);
    expect(onCancel).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it('ignores Escape key and backdrop click while busy', () => {
    const onCancel = vi.fn();
    render(<ApprovePluginDialog plugin={plugin} busy onConfirm={vi.fn()} onCancel={onCancel} />);
    const dialog = screen.getByRole('dialog', { name: /Approve Hello Plugin/i });

    fireEvent.click(dialog.parentElement!);
    expect(onCancel).not.toHaveBeenCalled();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('resynchronizes selected permissions when plugin prop changes', () => {
    const { rerender } = render(
      <ApprovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={vi.fn()} />
    );
    expect(screen.getByLabelText(/Publish items/i)).toBeChecked();
    expect(screen.getByLabelText(/Resolve playback/i)).toBeChecked();

    const otherPlugin: PluginRecord = {
      ...plugin,
      id: 'other',
      name: 'Other Plugin',
      manifest: {
        ...plugin.manifest,
        id: 'other',
        name: 'Other Plugin',
        permissions: ['items:publish'],
      },
    };
    rerender(<ApprovePluginDialog plugin={otherPlugin} onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole('dialog', { name: /Approve Other Plugin/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/Publish items/i)).toBeChecked();
    expect(screen.queryByLabelText(/Resolve playback/i)).not.toBeInTheDocument();
  });

  it('renders the error prop in a role="alert" element and renders nothing when absent', () => {
    const { rerender } = render(
      <ApprovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={vi.fn()} />
    );
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    rerender(
      <ApprovePluginDialog
        plugin={plugin}
        error="Permission denied by server"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Permission denied by server');
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

  it('closes on Escape key and backdrop click, but not on click inside dialog', () => {
    const onCancel = vi.fn();
    render(<RemovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={onCancel} />);
    const dialog = screen.getByRole('dialog', { name: /Remove Hello Plugin/i });

    fireEvent.click(dialog);
    expect(onCancel).not.toHaveBeenCalled();

    fireEvent.click(dialog.parentElement!);
    expect(onCancel).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it('ignores Escape key and backdrop click while busy', () => {
    const onCancel = vi.fn();
    render(<RemovePluginDialog plugin={plugin} busy onConfirm={vi.fn()} onCancel={onCancel} />);
    const dialog = screen.getByRole('dialog', { name: /Remove Hello Plugin/i });

    fireEvent.click(dialog.parentElement!);
    expect(onCancel).not.toHaveBeenCalled();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('resynchronizes purge state when plugin prop changes', () => {
    const { rerender } = render(
      <RemovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={vi.fn()} />
    );
    const purge = screen.getByLabelText(/Also delete this plugin's libraries and items/i);
    fireEvent.click(purge);
    expect(purge).toBeChecked();

    rerender(
      <RemovePluginDialog
        plugin={{ ...plugin, id: 'other', name: 'Other' }}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(screen.getByLabelText(/Also delete this plugin's libraries and items/i)).not.toBeChecked();
  });

  it('renders the error prop in a role="alert" element and renders nothing when absent', () => {
    const { rerender } = render(
      <RemovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={vi.fn()} />
    );
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    rerender(
      <RemovePluginDialog
        plugin={plugin}
        error="Cannot remove active plugin"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Cannot remove active plugin');
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

  it('closes on Escape key and backdrop click, but not on click inside dialog', async () => {
    const onClose = vi.fn();
    render(<ConnectionDialog plugin={plugin} onClose={onClose} />);
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    const dialog = screen.getByRole('dialog', { name: /Connection details for Hello Plugin/i });

    fireEvent.click(dialog);
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.click(dialog.parentElement!);
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('resets connection and token when rerendered with a different plugin', async () => {
    vi.spyOn(api, 'getPluginConnection').mockImplementation(async (id: string) => {
      if (id === 'other') {
        return { base_url: 'http://127.0.0.1:9200', secret: 'OTHER-SECRET' };
      }
      return { base_url: 'http://127.0.0.1:9100', secret: 'SECRET-VALUE' };
    });

    const { rerender } = render(
      <ConnectionDialog plugin={{ ...plugin, status: 'enabled' }} onClose={vi.fn()} />
    );
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Generate token/i }));
    await waitFor(() => expect(screen.getByDisplayValue('TOKEN-VALUE')).toBeInTheDocument());
    expect(screen.getByTestId('connection-env')).toHaveTextContent('KADR_PLUGIN_TOKEN=TOKEN-VALUE');

    // Rerender with different plugin
    rerender(
      <ConnectionDialog
        plugin={{ ...plugin, id: 'other', name: 'Other Plugin', status: 'enabled' }}
        onClose={vi.fn()}
      />
    );

    // Old token and secret must be gone, new secret loaded, token reset
    await waitFor(() => expect(screen.getByDisplayValue('OTHER-SECRET')).toBeInTheDocument());
    expect(screen.queryByDisplayValue('TOKEN-VALUE')).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue('SECRET-VALUE')).not.toBeInTheDocument();
    const env = screen.getByTestId('connection-env');
    expect(env.textContent).toContain('KADR_PLUGIN_SECRET=OTHER-SECRET');
    expect(env.textContent).not.toContain('TOKEN-VALUE');
    expect(env.textContent).toContain('KADR_PLUGIN_TOKEN=<click "Generate token">');
    expect(api.getPluginConnection).toHaveBeenCalledWith('other');
  });

  it('handles clipboard failure and clears error on subsequent success', async () => {
    render(<ConnectionDialog plugin={plugin} onClose={vi.fn()} />);
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());

    writeText.mockRejectedValueOnce(new Error('Clipboard denied'));
    fireEvent.click(screen.getByRole('button', { name: /Copy secret/i }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent('Copy failed. Select the text and copy it manually.');
    });
    expect(screen.queryByText(/Copied/i)).not.toBeInTheDocument();

    // Now copy succeeds
    writeText.mockResolvedValueOnce(undefined);
    fireEvent.click(screen.getByRole('button', { name: /Copy secret/i }));
    await waitFor(() => expect(screen.getByText(/Copied secret/i)).toBeInTheDocument());
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows API error message when token generation fails', async () => {
    vi.spyOn(api, 'issuePluginToken').mockRejectedValue(
      new ApiError(400, 'INVALID', { message: 'Failed to issue token for plugin' })
    );
    render(<ConnectionDialog plugin={{ ...plugin, status: 'enabled' }} onClose={vi.fn()} />);
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Generate token/i }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent('Failed to issue token for plugin');
    });
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
