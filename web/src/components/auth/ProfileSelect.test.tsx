import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ProfileSelect } from './ProfileSelect';
import { api } from '../../api/client';
import type { User } from '../../types';

describe('ProfileSelect Component', () => {
  const mockProfiles: User[] = [
    { id: 'admin-1', username: 'admin', role: 'admin' },
    { id: 'user-2', username: 'family', role: 'standard' },
  ];

  const onSuccess = vi.fn();
  const onCancel = vi.fn();
  const onSignOut = vi.fn();

  beforeEach(() => {
    vi.restoreAllMocks();
    onSuccess.mockClear();
    onCancel.mockClear();
    onSignOut.mockClear();
  });

  it('renders "Who\'s watching?" heading and profile list from api', async () => {
    vi.spyOn(api, 'getProfiles').mockResolvedValueOnce(mockProfiles);

    render(<ProfileSelect onSuccess={onSuccess} />);

    expect(screen.getByText(/Who's watching\?/i)).toBeDefined();

    await waitFor(() => {
      expect(screen.getByText('admin')).toBeDefined();
      expect(screen.getByText('family')).toBeDefined();
    });
  });

  it('falls back to seeded admin account if api.getProfiles fails', async () => {
    vi.spyOn(api, 'getProfiles').mockRejectedValueOnce(new Error('Network error'));

    render(<ProfileSelect onSuccess={onSuccess} />);

    await waitFor(() => {
      expect(screen.getByText('admin')).toBeDefined();
    });
  });

  it('opens PinKeypad when clicking a profile', async () => {
    vi.spyOn(api, 'getProfiles').mockResolvedValueOnce(mockProfiles);

    render(<ProfileSelect onSuccess={onSuccess} />);

    await waitFor(() => {
      expect(screen.getByText('admin')).toBeDefined();
    });

    fireEvent.click(screen.getByText('admin'));

    // Should now show PIN keypad for admin
    expect(screen.getByTestId('pin-keypad-container')).toBeDefined();
    expect(screen.getByText(/Enter your 4-digit PIN/i)).toBeDefined();

    // Clicking "Switch Profile" or Cancel in keypad returns to profile list
    fireEvent.click(screen.getByText('Switch Profile'));
    expect(screen.queryByTestId('pin-keypad-container')).toBeNull();
    expect(screen.getByText(/Who's watching\?/i)).toBeDefined();
  });

  it('renders active badge and triggers onCancel and onSignOut when currentUser is provided', async () => {
    vi.spyOn(api, 'getProfiles').mockResolvedValueOnce(mockProfiles);

    render(
      <ProfileSelect
        currentUser={mockProfiles[0]}
        onSuccess={onSuccess}
        onCancel={onCancel}
        onSignOut={onSignOut}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Active')).toBeDefined();
    });

    const cancelBtn = screen.getByRole('button', { name: /cancel/i });
    fireEvent.click(cancelBtn);
    expect(onCancel).toHaveBeenCalledTimes(1);

    const signOutBtn = screen.getByRole('button', { name: /sign out/i });
    fireEvent.click(signOutBtn);
    expect(onSignOut).toHaveBeenCalledTimes(1);
  });
});
