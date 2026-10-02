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
  'from-rose-500 via-rose-600 to-amber-500',
  'from-indigo-500 via-purple-600 to-pink-500',
  'from-cyan-500 via-teal-600 to-emerald-500',
  'from-amber-500 via-orange-600 to-red-500',
  'from-violet-500 via-indigo-600 to-blue-500',
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
      <div className="w-full flex flex-col items-center justify-center py-6 px-4">
        <button
          onClick={() => setSelectedUser(null)}
          className="mb-6 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-zinc-400 hover:text-white transition-colors"
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
    <div className="w-full max-w-4xl mx-auto flex flex-col items-center justify-center py-8 px-4 sm:px-6">
      {/* Title Header */}
      <div className="text-center mb-10">
        <h2 className="text-3xl sm:text-4xl font-extrabold tracking-tight text-white mb-2">
          Who&apos;s watching?
        </h2>
        <p className="text-sm text-zinc-400 max-w-md mx-auto">
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
              className="group flex flex-col items-center p-3 rounded-2xl focus:outline-none transition-all duration-200"
            >
              {/* Profile Avatar Card */}
              <div
                className={`relative h-24 w-24 sm:h-28 sm:w-28 rounded-2xl p-1 bg-gradient-to-tr ${gradient} shadow-lg shadow-zinc-950/80 group-hover:scale-105 group-hover:shadow-rose-950/40 group-focus:ring-2 group-focus:ring-rose-500 transition-all duration-200`}
              >
                <div className="h-full w-full rounded-xl bg-zinc-950 flex flex-col items-center justify-center overflow-hidden">
                  <span className="text-2xl sm:text-3xl font-black uppercase tracking-wider text-white group-hover:scale-110 transition-transform duration-200">
                    {profile.username.slice(0, 2) || 'U'}
                  </span>
                </div>

                {/* Role badge */}
                {profile.role === 'admin' && (
                  <div
                    title="Administrator Profile"
                    className="absolute -top-1.5 -right-1.5 bg-rose-600 text-white p-1 rounded-full shadow-md border border-zinc-900"
                  >
                    <Shield className="h-3 w-3" />
                  </div>
                )}
              </div>

              {/* Username & Metadata */}
              <div className="mt-3 text-center">
                <span className="block text-sm sm:text-base font-semibold text-zinc-300 group-hover:text-white transition-colors">
                  {profile.username}
                </span>

                {isCurrent && (
                  <span className="mt-0.5 inline-block text-[10px] font-semibold text-rose-400 uppercase tracking-wider bg-rose-500/10 px-2 py-0.5 rounded border border-rose-500/20">
                    Active
                  </span>
                )}
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
            className="px-5 py-2 rounded-xl text-xs font-semibold text-zinc-400 hover:text-white hover:bg-zinc-800/80 border border-zinc-800 transition-all"
          >
            Cancel
          </button>
        )}

        {currentUser && onSignOut && (
          <button
            onClick={onSignOut}
            className="flex items-center gap-2 px-5 py-2 rounded-xl text-xs font-semibold text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 border border-rose-500/20 transition-all"
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
