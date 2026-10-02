import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PinKeypad } from './PinKeypad';
import { api } from '../../api/client';
import type { User } from '../../types';

describe('PinKeypad Component', () => {
  const mockUser: User = {
    id: 'admin-1',
    username: 'admin',
    role: 'admin',
  };

  const onSuccess = vi.fn();
  const onCancel = vi.fn();

  beforeEach(() => {
    vi.restoreAllMocks();
    onSuccess.mockClear();
    onCancel.mockClear();
  });

  it('renders user info, 4 PIN dots, 0-9 numeric buttons, backspace, and cancel buttons', () => {
    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    // Shows user name
    expect(screen.getByText('admin')).toBeDefined();

    // 4 indicator dots
    const dots = screen.getAllByTestId(/^pin-dot-/);
    expect(dots).toHaveLength(4);

    // 0-9 buttons
    for (let i = 0; i <= 9; i++) {
      expect(screen.getByRole('button', { name: String(i) })).toBeDefined();
    }

    // Backspace button
    expect(screen.getByRole('button', { name: /backspace/i })).toBeDefined();

    // Cancel button
    expect(screen.getByRole('button', { name: /cancel/i })).toBeDefined();
  });

  it('fills indicator dots when numeric buttons are clicked', () => {
    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    const dot0 = screen.getByTestId('pin-dot-0');
    const dot1 = screen.getByTestId('pin-dot-1');

    expect(dot0.getAttribute('data-filled')).toBe('false');
    expect(dot1.getAttribute('data-filled')).toBe('false');

    fireEvent.click(screen.getByRole('button', { name: '1' }));
    expect(dot0.getAttribute('data-filled')).toBe('true');
    expect(dot1.getAttribute('data-filled')).toBe('false');

    fireEvent.click(screen.getByRole('button', { name: '2' }));
    expect(dot0.getAttribute('data-filled')).toBe('true');
    expect(dot1.getAttribute('data-filled')).toBe('true');
  });

  it('removes last entered digit when backspace button is clicked', () => {
    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    const dot0 = screen.getByTestId('pin-dot-0');
    const dot1 = screen.getByTestId('pin-dot-1');

    fireEvent.click(screen.getByRole('button', { name: '5' }));
    fireEvent.click(screen.getByRole('button', { name: '6' }));
    expect(dot0.getAttribute('data-filled')).toBe('true');
    expect(dot1.getAttribute('data-filled')).toBe('true');

    fireEvent.click(screen.getByRole('button', { name: /backspace/i }));
    expect(dot0.getAttribute('data-filled')).toBe('true');
    expect(dot1.getAttribute('data-filled')).toBe('false');

    fireEvent.click(screen.getByRole('button', { name: /backspace/i }));
    expect(dot0.getAttribute('data-filled')).toBe('false');
    expect(dot1.getAttribute('data-filled')).toBe('false');
  });

  it('handles physical keyboard input (0-9, Backspace, Escape)', () => {
    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    const dot0 = screen.getByTestId('pin-dot-0');
    const dot1 = screen.getByTestId('pin-dot-1');

    fireEvent.keyDown(window, { key: '7' });
    expect(dot0.getAttribute('data-filled')).toBe('true');

    fireEvent.keyDown(window, { key: '8' });
    expect(dot1.getAttribute('data-filled')).toBe('true');

    fireEvent.keyDown(window, { key: 'Backspace' });
    expect(dot1.getAttribute('data-filled')).toBe('false');
    expect(dot0.getAttribute('data-filled')).toBe('true');

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when Cancel button is clicked', () => {
    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('auto-submits when 4th digit is entered and calls onSuccess on valid PIN', async () => {
    vi.spyOn(api, 'loginWithPin').mockResolvedValueOnce({
      token: 'valid-jwt-token',
      user: mockUser,
    });

    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: '1' }));
    fireEvent.click(screen.getByRole('button', { name: '2' }));
    fireEvent.click(screen.getByRole('button', { name: '3' }));
    expect(api.loginWithPin).not.toHaveBeenCalled();

    // 4th digit triggers auto-submit
    fireEvent.click(screen.getByRole('button', { name: '4' }));

    await waitFor(() => {
      expect(api.loginWithPin).toHaveBeenCalledWith('1234', 'admin-1');
      expect(onSuccess).toHaveBeenCalledWith('valid-jwt-token', mockUser);
    });
  });

  it('displays error message and shake animation when PIN authentication fails', async () => {
    vi.spyOn(api, 'loginWithPin').mockRejectedValueOnce(
      new Error('Invalid user or PIN')
    );

    render(
      <PinKeypad
        user={mockUser}
        onSuccess={onSuccess}
        onCancel={onCancel}
      />
    );

    // Enter incorrect 4-digit PIN
    fireEvent.click(screen.getByRole('button', { name: '9' }));
    fireEvent.click(screen.getByRole('button', { name: '9' }));
    fireEvent.click(screen.getByRole('button', { name: '9' }));
    fireEvent.click(screen.getByRole('button', { name: '9' }));

    await waitFor(() => {
      expect(screen.getByText('Invalid user or PIN')).toBeDefined();
    });

    // Verify shake animation container
    const container = screen.getByTestId('pin-keypad-container');
    expect(container.className).toContain('animate-shake');

    // PIN is cleared so user can retry
    const dot0 = screen.getByTestId('pin-dot-0');
    expect(dot0.getAttribute('data-filled')).toBe('false');
  });
});
