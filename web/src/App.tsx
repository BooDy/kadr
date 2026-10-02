import { useState, useEffect, type FC } from 'react';
import {
  Film,
  Home,
  Tv,
  Layout,
  Activity,
  User as UserIcon,
  LogOut,
  Clapperboard,
} from 'lucide-react';
import { api } from './api/client';
import type { User } from './types';

export type NavView = 'home' | 'movies' | 'shows' | 'studio' | 'telemetry';

export const App: FC = () => {
  const [currentView, setCurrentView] = useState<NavView>('home');
  const [currentUser, setCurrentUser] = useState<User | null>(() => api.getUser());
  const [isAuthModalOpen, setIsAuthModalOpen] = useState(false);

  useEffect(() => {
    // Sync initial user state from local storage or verify session
    const user = api.getUser();
    if (user) {
      setCurrentUser(user);
    }
  }, []);

  const handleLogout = () => {
    api.logout();
    setCurrentUser(null);
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
            </nav>
          </div>

          {/* User Profile / Auth Area */}
          <div className="flex items-center gap-3">
            {currentUser ? (
              <div className="flex items-center gap-3">
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
                <button
                  onClick={handleLogout}
                  title="Sign out"
                  className="p-2 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/80 transition-colors"
                >
                  <LogOut className="h-4 w-4" />
                </button>
              </div>
            ) : (
              <button
                onClick={() => setIsAuthModalOpen(true)}
                className="flex items-center gap-2 rounded-lg bg-rose-600 px-3.5 py-1.5 text-sm font-semibold text-white shadow-sm hover:bg-rose-500 transition-colors"
              >
                <UserIcon className="h-4 w-4" />
                Sign In
              </button>
            )}
          </div>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 w-full mx-auto max-w-7xl px-4 sm:px-6 lg:px-8 py-8">
        {currentView === 'home' && (
          <div className="space-y-6">
            <div className="rounded-2xl border border-zinc-800/80 bg-gradient-to-b from-zinc-900/80 to-zinc-950/80 p-8 shadow-xl">
              <div className="flex items-center gap-3 text-rose-500 mb-2">
                <Clapperboard className="h-6 w-6" />
                <span className="text-xs font-bold tracking-widest uppercase">
                  Cinema Media Server
                </span>
              </div>
              <h1 className="text-3xl font-extrabold tracking-tight text-white sm:text-4xl">
                Welcome to Kadr
              </h1>
              <p className="mt-3 text-base text-zinc-400 max-w-2xl">
                Declarative, hardware-accelerated media streaming with native WebVTT subtitles, dynamic screen layouts, and real-time telemetry.
              </p>
            </div>
          </div>
        )}

        {currentView === 'movies' && (
          <div className="space-y-4">
            <h2 className="text-2xl font-bold tracking-tight text-white">Movies Catalog</h2>
            <p className="text-zinc-400 text-sm">Browse movies library and high-definition titles.</p>
          </div>
        )}

        {currentView === 'shows' && (
          <div className="space-y-4">
            <h2 className="text-2xl font-bold tracking-tight text-white">TV Shows</h2>
            <p className="text-zinc-400 text-sm">Follow your favorite TV series and episodes.</p>
          </div>
        )}

        {currentView === 'studio' && (
          <div className="space-y-4">
            <h2 className="text-2xl font-bold tracking-tight text-white">Layout Studio</h2>
            <p className="text-zinc-400 text-sm">Design and preview declarative screen ASTs.</p>
          </div>
        )}

        {currentView === 'telemetry' && (
          <div className="space-y-4">
            <h2 className="text-2xl font-bold tracking-tight text-white">System Telemetry</h2>
            <p className="text-zinc-400 text-sm">Monitor system memory, SQLite WAL, and active playback sessions.</p>
          </div>
        )}

        {/* Authentication Modal Slot (wired in Task 2) */}
        {isAuthModalOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
            <div className="relative w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl">
              <h3 className="text-lg font-semibold text-white mb-2">Sign In</h3>
              <p className="text-sm text-zinc-400 mb-6">Enter your 4-digit profile PIN to authenticate.</p>
              <div className="flex justify-end gap-3">
                <button
                  onClick={() => setIsAuthModalOpen(false)}
                  className="px-4 py-2 rounded-lg text-sm text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors"
                >
                  Close
                </button>
              </div>
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
