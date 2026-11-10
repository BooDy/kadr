import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { PluginList } from './PluginList';
import { RegisterPluginForm } from './RegisterPluginForm';
import type { PluginRecord, PluginStatus } from '../../types';

const make = (id: string, status: PluginStatus, granted: PluginRecord['granted_permissions'] = []): PluginRecord => ({
  id,
  name: `Plugin ${id}`,
  base_url: `http://127.0.0.1:9${id.length}00`,
  manifest: {
    id,
    name: `Plugin ${id}`,
    version: '1.2.3',
    api_version: 1,
    permissions: ['items:publish', 'playback:resolve'],
    capabilities: { virtual_library: true, playback_resolver: true },
  },
  granted_permissions: granted,
  status,
  created_at: 1,
});

const handlers = () => ({
  onApprove: vi.fn(),
  onEnable: vi.fn(),
  onDisable: vi.fn(),
  onConnection: vi.fn(),
  onRemove: vi.fn(),
});

describe('PluginList', () => {
  it('shows an empty state', () => {
    render(<PluginList plugins={[]} {...handlers()} />);
    expect(screen.getByText(/No plugins registered yet/i)).toBeInTheDocument();
  });

  it('renders name, version, status and granted permissions', () => {
    render(
      <PluginList
        plugins={[make('a', 'enabled', ['items:publish'])]}
        {...handlers()}
      />
    );
    const row = screen.getByTestId('plugin-row-a');
    expect(within(row).getByText('Plugin a')).toBeInTheDocument();
    expect(within(row).getByText(/v1\.2\.3/)).toBeInTheDocument();
    expect(within(row).getByText(/Enabled/)).toBeInTheDocument();
    expect(within(row).getByText(/items:publish/)).toBeInTheDocument();
    expect(within(row).queryByText(/playback:resolve/)).toBeNull();
  });

  it('shows the requested permissions for pending plugins', () => {
    render(<PluginList plugins={[make('p', 'pending')]} {...handlers()} />);
    const row = screen.getByTestId('plugin-row-p');
    expect(within(row).getByText(/Awaiting approval/i)).toBeInTheDocument();
    expect(within(row).getByText(/items:publish/)).toBeInTheDocument();
    expect(within(row).getByText(/playback:resolve/)).toBeInTheDocument();
  });

  it('offers the right actions for each status', () => {
    const plugins = [
      make('p', 'pending'),
      make('e', 'enabled'),
      make('u', 'unhealthy'),
      make('d', 'disabled'),
    ];
    render(<PluginList plugins={plugins} {...handlers()} />);
    const names = (id: string) =>
      within(screen.getByTestId(`plugin-row-${id}`))
        .getAllByRole('button')
        .map((b) => b.textContent?.trim());

    expect(names('p')).toEqual(['Review & approve', 'Remove']);
    expect(names('e')).toEqual(['Connection details', 'Disable', 'Remove']);
    expect(names('u')).toEqual(['Connection details', 'Disable', 'Remove']);
    expect(names('d')).toEqual(['Enable', 'Connection details', 'Remove']);
  });

  it('invokes the matching handler with the plugin', () => {
    const h = handlers();
    const plugins = [make('p', 'pending'), make('e', 'enabled'), make('d', 'disabled')];
    render(<PluginList plugins={plugins} {...h} />);

    fireEvent.click(within(screen.getByTestId('plugin-row-p')).getByRole('button', { name: /Review & approve/i }));
    expect(h.onApprove).toHaveBeenCalledWith(plugins[0]);
    fireEvent.click(within(screen.getByTestId('plugin-row-e')).getByRole('button', { name: /^Disable$/i }));
    expect(h.onDisable).toHaveBeenCalledWith(plugins[1]);
    fireEvent.click(within(screen.getByTestId('plugin-row-e')).getByRole('button', { name: /Connection details/i }));
    expect(h.onConnection).toHaveBeenCalledWith(plugins[1]);
    fireEvent.click(within(screen.getByTestId('plugin-row-d')).getByRole('button', { name: /^Enable$/i }));
    expect(h.onEnable).toHaveBeenCalledWith(plugins[2]);
    fireEvent.click(within(screen.getByTestId('plugin-row-d')).getByRole('button', { name: /^Remove$/i }));
    expect(h.onRemove).toHaveBeenCalledWith(plugins[2]);
  });

  it('disables every action while busy', () => {
    render(<PluginList plugins={[make('e', 'enabled')]} busy {...handlers()} />);
    for (const button of within(screen.getByTestId('plugin-row-e')).getAllByRole('button')) {
      expect(button).toBeDisabled();
    }
  });
});

describe('RegisterPluginForm', () => {
  it('submits the trimmed URL and clears the field on success', async () => {
    const onRegister = vi.fn().mockResolvedValue(true);
    render(<RegisterPluginForm onRegister={onRegister} />);
    const input = screen.getByLabelText(/Plugin base URL/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: '  http://127.0.0.1:9100  ' } });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(onRegister).toHaveBeenCalledWith('http://127.0.0.1:9100'));
    await waitFor(() => expect(input.value).toBe(''));
  });

  it('keeps the field when registration fails', async () => {
    const onRegister = vi.fn().mockResolvedValue(false);
    render(<RegisterPluginForm onRegister={onRegister} />);
    const input = screen.getByLabelText(/Plugin base URL/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'http://bad' } });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(onRegister).toHaveBeenCalled());
    expect(input.value).toBe('http://bad');
  });

  it('ignores empty input and respects disabled', () => {
    const onRegister = vi.fn();
    const { rerender } = render(<RegisterPluginForm onRegister={onRegister} />);
    expect(screen.getByRole('button', { name: /Register plugin/i })).toBeDisabled();
    fireEvent.submit(screen.getByRole('button', { name: /Register plugin/i }).closest('form')!);
    expect(onRegister).not.toHaveBeenCalled();

    rerender(<RegisterPluginForm onRegister={onRegister} disabled />);
    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), { target: { value: 'http://x' } });
    expect(screen.getByRole('button', { name: /Register plugin/i })).toBeDisabled();
    expect(screen.getByLabelText(/Plugin base URL/i)).toBeDisabled();
  });
});
