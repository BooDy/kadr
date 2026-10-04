import { useState, useEffect, type FC } from 'react';
import { LogOut, ArrowLeft, Shield } from 'lucide-react';
import { api } from '../../api/client';
import type { User } from '../../types';
import { PinKeypad } from './PinKeypad';

export interface ProfileSelectProps {
  onSuccess: (token: string, user: User) => void;
  currentUser?: User | null;
  onCancel?: () => void;
  onSignOut?: () => void;
}

const DEFAULT_ADMIN: User = {
  id: 'admin-1',
  username: 'admin',
  role: 'admin',
};

// Distinct gradients for profile avatars
const AVATAR_GRADIENTS = [
  'from-accent via-cta to-highlight',
  'from-highlight via-accent to-cta',
  'from-cta via-highlight to-accent',
  'from-accent to-highlight',
  'from-cta to-accent',
];

export const ProfileSelect: FC<ProfileSelectProps> = ({
  onSuccess,
  currentUser,
  onCancel,
  onSignOut,
}) => {
  const [profiles, setProfiles] = useState<User[]>([DEFAULT_ADMIN]);
  const [selectedUser, setSelectedUser] = useState<User | null>(null);

  useEffect(() => {
    let isMounted = true;

    async function loadProfiles() {
      try {
        const fetched = await api.getProfiles();
        if (isMounted && fetched && fetched.length > 0) {
          setProfiles(fetched);
        }
      } catch {
        // Fall back to default admin account if backend is unavailable
      }
    }

    loadProfiles();

    return () => {
      isMounted = false;
    };
  }, []);

  // When a profile is selected, show the PIN keypad
  if (selectedUser) {
    return (
      <div className="w-full flex flex-col items-center justify-center py-6 px-4 bg-canvas text-text-main">
        <button
          onClick={() => setSelectedUser(null)}
          className="mb-6 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted hover:text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none rounded-xl px-3 py-1.5"
        >
          <ArrowLeft className="h-4 w-4" />
          Switch Profile
        </button>

        <PinKeypad
          user={selectedUser}
          onSuccess={onSuccess}
          onCancel={() => setSelectedUser(null)}
        />
      </div>
    );
  }

  return (
    <div className="w-full max-w-4xl mx-auto flex flex-col items-center justify-center py-8 px-4 bg-canvas text-text-main">
      {/* Title Header */}
      <div className="text-center mb-10 flex flex-col items-center">
        <img
          src="/kadr-logo-ui.png"
          alt="KADR Logo"
          className="h-20 w-20 object-contain drop-shadow-2xl mb-4 hover:scale-105 transition-transform duration-200"
        />
        <h2 className="text-3xl sm:text-4xl font-extrabold tracking-tight text-text-main mb-2">
          Who&apos;s watching?
        </h2>
        <p className="text-sm text-muted max-w-md mx-auto">
          Select your profile to unlock custom layouts, personalized carousels, and resume playback.
        </p>
      </div>

      {/* Profiles Grid */}
      <div className="flex flex-wrap items-center justify-center gap-6 sm:gap-8 mb-12">
        {profiles.map((profile, idx) => {
          const gradient = AVATAR_GRADIENTS[idx % AVATAR_GRADIENTS.length];
          const isCurrent = currentUser?.id === profile.id;

          return (
            <button
              key={profile.id}
              onClick={() => setSelectedUser(profile)}
              className="group flex flex-col items-center rounded-xl p-1 bg-panel hover:bg-panel-hover border border-border-subtle transition-all duration-200 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <div className="p-3 flex flex-col items-center">
                {/* Profile Avatar Card */}
                <div
                  className={`relative h-24 w-24 sm:h-28 sm:w-28 rounded-xl p-0.5 bg-gradient-to-tr ${gradient} shadow-md group-hover:scale-105 transition-transform duration-200`}
                >
                  <div className="h-full w-full rounded-[10px] bg-canvas flex flex-col items-center justify-center overflow-hidden">
                    <span className="text-2xl sm:text-3xl font-black uppercase tracking-wider text-text-main group-hover:scale-110 transition-transform duration-200">
                      {profile.username.slice(0, 2) || 'U'}
                    </span>
                  </div>

                  {/* Role badge */}
                  {profile.role === 'admin' && (
                    <div
                      title="Administrator Profile"
                      className="absolute -top-1.5 -right-1.5 bg-cta text-white p-1 rounded-full shadow-md border border-border-subtle"
                    >
                      <Shield className="h-3 w-3" />
                    </div>
                  )}
                </div>

                {/* Username & Metadata */}
                <div className="mt-3 text-center flex flex-col items-center gap-1.5">
                  <span className="block text-sm sm:text-base font-semibold text-text-main group-hover:text-white transition-colors">
                    {profile.username}
                  </span>

                  <div className="flex items-center gap-1.5">
                    {profile.role === 'admin' ? (
                      <span className="inline-block text-[10px] font-semibold uppercase tracking-wider bg-cta text-white px-2 py-0.5 rounded-full shadow-xs">
                        Admin
                      </span>
                    ) : (
                      <span className="inline-block text-[10px] font-semibold uppercase tracking-wider bg-panel text-muted border border-border-subtle px-2 py-0.5 rounded-full">
                        Standard
                      </span>
                    )}

                    {isCurrent && (
                      <span className="inline-block text-[10px] font-semibold text-accent uppercase tracking-wider bg-accent/15 px-2 py-0.5 rounded border border-accent/30">
                        Active
                      </span>
                    )}
                  </div>
                </div>
              </div>
            </button>
          );
        })}
      </div>

      {/* Footer controls (for Quick Switch or Sign Out) */}
      <div className="flex items-center gap-4">
        {currentUser && onCancel && (
          <button
            onClick={onCancel}
            className="px-5 py-2 rounded-xl text-xs font-semibold text-muted hover:text-text-main hover:bg-canvas/40 border border-border-subtle transition-all focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            Cancel
          </button>
        )}

        {currentUser && onSignOut && (
          <button
            onClick={onSignOut}
            className="flex items-center gap-2 px-5 py-2 rounded-xl text-xs font-semibold text-muted hover:text-text-main hover:bg-canvas/40 border border-border-subtle transition-all focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <LogOut className="h-3.5 w-3.5" />
            Sign Out
          </button>
        )}
      </div>
    </div>
  );
};

export default ProfileSelect;
