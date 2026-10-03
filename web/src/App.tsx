import { useState, useEffect, type FC } from 'react';
import {
  Film,
  Home,
  Tv,
  Layout,
  Activity,
  LogOut,
  Users,
  Settings,
} from 'lucide-react';
import { api } from './api/client';
import type { User } from './types';
import { ProfileSelect } from './components/auth/ProfileSelect';
import { BrowseScreen } from './components/browse/BrowseScreen';
import { CinemaPlayer } from './components/player/CinemaPlayer';
import { LayoutStudio } from './components/studio/LayoutStudio';
import { TelemetryDashboard } from './components/telemetry/TelemetryDashboard';
import { AdminDashboard } from './components/admin/AdminDashboard';

export type NavView = 'home' | 'movies' | 'shows' | 'studio' | 'telemetry' | 'admin' | 'player';

export const App: FC = () => {
  const [currentView, setCurrentView] = useState<NavView>('home');
  const [currentUser, setCurrentUser] = useState<User | null>(() => api.getUser());
  const [isAuthModalOpen, setIsAuthModalOpen] = useState(false);
  const [activePlayingItemId, setActivePlayingItemId] = useState<number | null>(null);

  useEffect(() => {
    // Sync initial user state from local storage or verify session
    const user = api.getUser();
    if (user) {
      setCurrentUser(user);
    }
  }, []);

  const handleAuthSuccess = (token: string, user: User) => {
    api.setToken(token);
    api.setUser(user);
    setCurrentUser(user);
    setIsAuthModalOpen(false);
  };

  const handleLogout = () => {
    api.logout();
    setCurrentUser(null);
    setIsAuthModalOpen(false);
  };

  const handlePlayItem = (itemId: number) => {
    setActivePlayingItemId(itemId);
    setCurrentView('player');
  };

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col font-sans selection:bg-rose-500 selection:text-white">
      {/* Cinema App Header */}
      <header className="sticky top-0 z-40 w-full border-b border-zinc-800/80 bg-zinc-950/90 backdrop-blur-md">
        <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-4 sm:px-6 lg:px-8">
          {/* Logo & Brand */}
          <div className="flex items-center gap-8">
            <button
              onClick={() => setCurrentView('home')}
              className="flex items-center gap-2.5 text-left group focus:outline-none"
            >
              <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-gradient-to-br from-rose-500 to-rose-700 shadow-md shadow-rose-900/30 group-hover:scale-105 transition-transform duration-200">
                <Film className="h-5 w-5 text-white" />
              </div>
              <span className="text-xl font-bold tracking-tight text-white group-hover:text-rose-400 transition-colors">
                KADR
              </span>
            </button>

            {/* Navigation Links */}
            <nav className="hidden md:flex items-center gap-1 text-sm font-medium">
              <button
                onClick={() => setCurrentView('home')}
                className={`flex items-center gap-2 px-3 py-2 rounded-md transition-colors ${
                  currentView === 'home'
                    ? 'text-white bg-zinc-800/90 font-semibold'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900/60'
                }`}
              >
                <Home className="h-4 w-4" />
                Home
              </button>
              <button
                onClick={() => setCurrentView('movies')}
                className={`flex items-center gap-2 px-3 py-2 rounded-md transition-colors ${
                  currentView === 'movies'
                    ? 'text-white bg-zinc-800/90 font-semibold'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900/60'
                }`}
              >
                <Film className="h-4 w-4" />
                Movies
              </button>
              <button
                onClick={() => setCurrentView('shows')}
                className={`flex items-center gap-2 px-3 py-2 rounded-md transition-colors ${
                  currentView === 'shows'
                    ? 'text-white bg-zinc-800/90 font-semibold'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900/60'
                }`}
              >
                <Tv className="h-4 w-4" />
                Shows
              </button>
              <button
                onClick={() => setCurrentView('studio')}
                className={`flex items-center gap-2 px-3 py-2 rounded-md transition-colors ${
                  currentView === 'studio'
                    ? 'text-white bg-zinc-800/90 font-semibold'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900/60'
                }`}
              >
                <Layout className="h-4 w-4" />
                Studio
              </button>
              <button
                onClick={() => setCurrentView('telemetry')}
                className={`flex items-center gap-2 px-3 py-2 rounded-md transition-colors ${
                  currentView === 'telemetry'
                    ? 'text-white bg-zinc-800/90 font-semibold'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900/60'
                }`}
              >
                <Activity className="h-4 w-4" />
                Telemetry
              </button>
              {currentUser?.role === 'admin' && (
                <button
                  onClick={() => setCurrentView('admin')}
                  className={`flex items-center gap-2 px-3 py-2 rounded-md transition-colors ${
                    currentView === 'admin'
                      ? 'text-white bg-zinc-800/90 font-semibold'
                      : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900/60'
                  }`}
                >
                  <Settings className="h-4 w-4" />
                  Admin
                </button>
              )}
            </nav>
          </div>

          {/* User Profile / Auth Area */}
          <div className="flex items-center gap-3">
            {currentUser ? (
              <div className="flex items-center gap-2 sm:gap-3">
                {/* Active user avatar chip */}
                <div className="flex items-center gap-2.5 px-3 py-1.5 rounded-full bg-zinc-900 border border-zinc-800">
                  <div className="h-6 w-6 rounded-full bg-gradient-to-tr from-rose-500 to-amber-500 flex items-center justify-center text-xs font-bold text-white uppercase">
                    {currentUser.username[0] || 'U'}
                  </div>
                  <span className="text-xs font-medium text-zinc-300">
                    {currentUser.username}
                  </span>
                  {currentUser.role === 'admin' && (
                    <span className="rounded bg-rose-500/20 px-1.5 py-0.5 text-[10px] font-semibold text-rose-400 border border-rose-500/30">
                      ADMIN
                    </span>
                  )}
                </div>

                {/* Switch Profile Button */}
                <button
                  onClick={() => setIsAuthModalOpen(true)}
                  title="Switch Profile"
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-300 hover:text-white bg-zinc-900/80 hover:bg-zinc-800 border border-zinc-800 transition-colors"
                >
                  <Users className="h-3.5 w-3.5 text-zinc-400" />
                  <span className="hidden sm:inline">Switch Profile</span>
                </button>

                {/* Sign Out Button */}
                <button
                  onClick={handleLogout}
                  title="Sign out"
                  className="p-2 rounded-lg text-zinc-400 hover:text-rose-400 hover:bg-zinc-800/80 transition-colors"
                >
                  <LogOut className="h-4 w-4" />
                </button>
              </div>
            ) : null}
          </div>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 w-full mx-auto max-w-7xl px-4 sm:px-6 lg:px-8 py-8">
        {!currentUser ? (
          <div className="flex flex-col items-center justify-center min-h-[60vh]">
            <ProfileSelect onSuccess={handleAuthSuccess} />
          </div>
        ) : (
          <>
            {currentView === 'home' && (
              <BrowseScreen screenId="home" onPlayItem={handlePlayItem} />
            )}

            {currentView === 'movies' && (
              <BrowseScreen screenId="movies" onPlayItem={handlePlayItem} />
            )}

            {currentView === 'shows' && (
              <BrowseScreen screenId="shows" onPlayItem={handlePlayItem} />
            )}

            {currentView === 'player' && activePlayingItemId !== null && (
              <CinemaPlayer
                itemId={activePlayingItemId}
                onClose={() => {
                  setActivePlayingItemId(null);
                  setCurrentView('home');
                }}
              />
            )}

            {currentView === 'studio' && (
              <LayoutStudio onPlayItem={handlePlayItem} />
            )}

            {currentView === 'telemetry' && (
              <TelemetryDashboard />
            )}

            {currentView === 'admin' && currentUser?.role === 'admin' && (
              <AdminDashboard />
            )}
          </>
        )}

        {/* Switch Profile Modal (when already authenticated and user clicked Switch Profile) */}
        {currentUser && isAuthModalOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
            <div className="relative w-full max-w-2xl rounded-3xl border border-zinc-800 bg-zinc-950/95 p-6 sm:p-8 shadow-2xl backdrop-blur-md">
              <ProfileSelect
                currentUser={currentUser}
                onSuccess={handleAuthSuccess}
                onCancel={() => setIsAuthModalOpen(false)}
                onSignOut={handleLogout}
              />
            </div>
          </div>
        )}
      </main>

      {/* Cinema Footer */}
      <footer className="border-t border-zinc-900 py-6 text-center text-xs text-zinc-600">
        <p>Kadr Cinema Server &copy; 2026. Built with Vite, React & Axum.</p>
      </footer>
    </div>
  );
};

export default App;
