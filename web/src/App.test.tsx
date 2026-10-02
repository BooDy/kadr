import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import App from './App';
import { api } from './api/client';

describe('App Shell', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('renders KADR brand and navigation items', async () => {
    render(<App />);
    expect(screen.getByText('KADR')).toBeDefined();
    expect(screen.getByRole('button', { name: /home/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /movies/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /shows/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /studio/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /telemetry/i })).toBeDefined();
    await waitFor(() => expect(screen.getByText('admin')).toBeDefined());
  });

  it('shows ProfileSelect when no user is logged in', async () => {
    render(<App />);
    expect(screen.getByText(/Who's watching\?/i)).toBeDefined();
    await waitFor(() => expect(screen.getByText('admin')).toBeDefined());
  });

  it('navigates between views when user is authenticated', () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    render(<App />);
    expect(screen.getByText('Welcome to Kadr')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /movies/i }));
    expect(screen.getByText('Movies Catalog')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /shows/i }));
    expect(screen.getByText('TV Shows')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /studio/i }));
    expect(screen.getByText('Layout Studio')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /telemetry/i }));
    expect(screen.getByText('System Telemetry')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /home/i }));
    expect(screen.getByText('Welcome to Kadr')).toBeDefined();
  });

  it('shows active user avatar in header with Switch Profile menu and opens switch profile modal', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    render(<App />);
    expect(screen.getByText('admin')).toBeDefined();
    expect(screen.getByText('ADMIN')).toBeDefined();

    const switchBtn = screen.getByTitle('Switch Profile');
    expect(switchBtn).toBeDefined();

    fireEvent.click(switchBtn);

    // Profile selector should be opened in modal with active user indicator
    await waitFor(() => {
      expect(screen.getByText('Active')).toBeDefined();
    });

    // Close/Cancel switch profile modal
    const cancelBtn = screen.getByRole('button', { name: /cancel/i });
    fireEvent.click(cancelBtn);
    expect(screen.queryByText('Active')).toBeNull();
  });

  it('logs out when sign out is clicked and returns to ProfileSelect', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    render(<App />);
    expect(screen.getByText('admin')).toBeDefined();

    const signOutBtn = screen.getByTitle('Sign out');
    fireEvent.click(signOutBtn);

    expect(api.getToken()).toBeNull();
    expect(api.getUser()).toBeNull();

    // Now unauthenticated: ProfileSelect is shown
    await waitFor(() => expect(screen.getByText(/Who's watching\?/i)).toBeDefined());
  });
});
