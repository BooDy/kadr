import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import App from './App';
import { api } from './api/client';

describe('App Shell', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('renders KADR brand and navigation items', () => {
    render(<App />);
    expect(screen.getByText('KADR')).toBeDefined();
    expect(screen.getByRole('button', { name: /home/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /movies/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /shows/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /studio/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /telemetry/i })).toBeDefined();
  });

  it('navigates between views when clicking navigation buttons', () => {
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

  it('shows Sign In button when no user is logged in, and opens modal on click', () => {
    render(<App />);
    const signInBtn = screen.getByRole('button', { name: /sign in/i });
    expect(signInBtn).toBeDefined();

    fireEvent.click(signInBtn);
    expect(screen.getByText('Enter your 4-digit profile PIN to authenticate.')).toBeDefined();

    const closeBtn = screen.getByRole('button', { name: /close/i });
    fireEvent.click(closeBtn);
    expect(screen.queryByText('Enter your 4-digit profile PIN to authenticate.')).toBeNull();
  });

  it('shows user chip and logs out when user is authenticated', () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    render(<App />);
    expect(screen.getByText('admin')).toBeDefined();
    expect(screen.getByText('ADMIN')).toBeDefined();

    const signOutBtn = screen.getByTitle('Sign out');
    fireEvent.click(signOutBtn);

    expect(api.getToken()).toBeNull();
    expect(api.getUser()).toBeNull();
    expect(screen.getByRole('button', { name: /sign in/i })).toBeDefined();
  });
});
