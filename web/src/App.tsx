import { useState, useEffect, useCallback, type FC } from 'react';
import {
  Film,
  Home,
  Tv,
  Sparkles,
  Music,
  Video,
  BookOpen,
  Folder,
  LogOut,
  Users,
  Settings,
  Lock,
  Unlock,
} from 'lucide-react';
import { api } from './api/client';
import type { Library, MediaType, User } from './types';
import { ProfileSelect } from './components/auth/ProfileSelect';
import { PinKeypad } from './components/auth/PinKeypad';
import { BrowseScreen } from './components/browse/BrowseScreen';
import { CinemaPlayer } from './components/player/CinemaPlayer';
import { AdminDashboard } from './components/admin/AdminDashboard';

export type NavView = 'home' | 'movies' | 'shows' | 'studio' | 'telemetry' | 'admin' | 'player' | string;

const getLibraryIcon = (mediaType: MediaType | string) => {
  switch (mediaType) {
    case 'movie':
    case 'Movie' as unknown:
      return <Film className="h-4 w-4" />;
    case 'show':
    case 'Show' as unknown:
      return <Tv className="h-4 w-4" />;
    case 'anime':
    case 'Anime' as unknown:
      return <Sparkles className="h-4 w-4" />;
    case 'music':
    case 'Music' as unknown:
      return <Music className="h-4 w-4" />;
    case 'home_videos':
    case 'HomeVideos' as unknown:
      return <Video className="h-4 w-4" />;
    case 'audiobook':
    case 'Audiobook' as unknown:
      return <BookOpen className="h-4 w-4" />;
    default:
      return <Folder className="h-4 w-4" />;
  }
};

export const App: FC = () => {
  const [currentView, setCurrentView] = useState<NavView>('home');
  const [currentUser, setCurrentUser] = useState<User | null>(() => api.getUser());
  const [isAuthModalOpen, setIsAuthModalOpen] = useState(false);
  const [activePlayingItemId, setActivePlayingItemId] = useState<number | null>(null);

  // Library and unlock state
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [unlockedLibraryIds, setUnlockedLibraryIds] = useState<string[]>(() => api.getUnlockedLibraryIds());
  const [libraryToUnlock, setLibraryToUnlock] = useState<Library | null>(null);
  const [activeLibrary, setActiveLibrary] = useState<Library | null>(null);
  const [pendingFolderNav, setPendingFolderNav] = useState<{ libraryId: string; folderPath: string } | null>(null);
  const [activeFolderNav, setActiveFolderNav] = useState<{ libraryId: string; folderPath: string } | null>(null);

  const refreshLibraries = useCallback(() => {
    api.getLibraries().then(setLibraries).catch(() => []);
    setUnlockedLibraryIds(api.getUnlockedLibraryIds());
  }, []);

  useEffect(() => {
    const user = api.getUser();
    if (user) {
      setCurrentUser(user);
      refreshLibraries();
    }
  }, [refreshLibraries]);

  const handleAuthSuccess = (token: string, user: User) => {
    api.clearUnlockedTokens();
    setUnlockedLibraryIds([]);
    setActiveLibrary(null);
    setLibraryToUnlock(null);
    setPendingFolderNav(null);
    setActiveFolderNav(null);
    api.setToken(token);
    api.setUser(user);
    setCurrentUser(user);
    setIsAuthModalOpen(false);
    refreshLibraries();
  };

  const handleLogout = () => {
    api.logout();
    setCurrentUser(null);
    setUnlockedLibraryIds([]);
    setActiveLibrary(null);
    setLibraryToUnlock(null);
    setPendingFolderNav(null);
    setActiveFolderNav(null);
    setIsAuthModalOpen(false);
  };

  const handleSelectLibrary = (lib: Library) => {
    if (lib.is_private && !api.isLibraryUnlocked(lib.id) && !unlockedLibraryIds.includes(lib.id)) {
      setLibraryToUnlock(lib);
      return;
    }
    setActiveFolderNav(null);
    setActiveLibrary(lib);
    setCurrentView(`library-${lib.id}`);
  };

  const handleNavigate = (view: NavView) => {
    setActiveFolderNav(null);
    setActiveLibrary(null);
    setCurrentView(view);
  };

  const handleNavigateToFolder = (libraryId: string, folderPath: string) => {
    const lib = libraries.find((l) => l.id === libraryId);
    if (!lib) return;

    const isLocked = lib.is_private && !api.isLibraryUnlocked(lib.id) && !unlockedLibraryIds.includes(lib.id);
    if (isLocked) {
      setPendingFolderNav({ libraryId, folderPath });
      setLibraryToUnlock(lib);
      return;
    }

    setActiveFolderNav({ libraryId, folderPath });
    setActiveLibrary(lib);
    setCurrentView(`library-${lib.id}`);
  };

  const handleUnlockSuccess = (unlockedLib: Library) => {
    if (pendingFolderNav && pendingFolderNav.libraryId === unlockedLib.id) {
      setActiveFolderNav(pendingFolderNav);
      setPendingFolderNav(null);
    } else {
      setActiveFolderNav(null);
    }
    setActiveLibrary(unlockedLib);
    setCurrentView(`library-${unlockedLib.id}`);
    setLibraryToUnlock(null);
  };

  const handlePlayItem = (itemId: number) => {
    setActivePlayingItemId(itemId);
    setCurrentView('player');
  };

  const navItemClass = (isActive: boolean) =>
    `flex items-center gap-2 px-3 py-2 rounded-xl transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
      isActive
        ? 'text-accent bg-canvas/70 font-semibold border border-border-subtle'
        : 'text-muted hover:text-text-main hover:bg-panel-hover border border-transparent'
    }`;

  return (
    <div className="min-h-screen bg-canvas text-text-main flex flex-col font-sans selection:bg-accent selection:text-white">
      {/* Cinema App Header */}
      <header className="sticky top-0 z-40 w-full border-b border-border-subtle bg-panel/95 backdrop-blur-md">
        <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-4 sm:px-6 lg:px-8">
          {/* Logo & Brand */}
          <div className="flex items-center gap-8">
            <button
              onClick={() => handleNavigate('home')}
              className="flex items-center gap-2.5 text-left group focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none rounded-xl"
            >
              <img
                src="/kadr-logo-ui.png"
                alt="KADR Logo"
                className="h-9 w-9 object-contain drop-shadow group-hover:scale-105 transition-transform duration-200"
              />
              <span className="text-xl font-bold tracking-tight text-text-main group-hover:text-accent transition-colors">
                KADR
              </span>
            </button>

            {/* Navigation Links */}
            <nav className="hidden md:flex items-center gap-1 text-sm font-medium">
              <button
                onClick={() => handleNavigate('home')}
                className={navItemClass(currentView === 'home')}
              >
                <Home className="h-4 w-4" />
                Home
              </button>

              {/* Dynamic Library Navigation Tabs */}
              {libraries.map((lib) => {
                const isUnlocked = unlockedLibraryIds.includes(lib.id) || api.isLibraryUnlocked(lib.id);
                const isCurrent = currentView === `library-${lib.id}`;
                return (
                  <button
                    key={lib.id}
                    onClick={() => handleSelectLibrary(lib)}
                    className={navItemClass(isCurrent)}
                  >
                    {lib.is_private ? (
                      isUnlocked ? (
                        <Unlock className="h-4 w-4 text-accent" />
                      ) : (
                        <Lock className="h-4 w-4 text-highlight" />
                      )
                    ) : (
                      getLibraryIcon(lib.media_type)
                    )}
                    <span>{lib.name}</span>
                    {lib.is_private && (
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded-md font-medium border flex items-center gap-1 ${
                          isUnlocked
                            ? 'bg-panel text-accent border-border-subtle'
                            : 'bg-panel text-highlight border-border-subtle'
                        }`}
                      >
                        {isUnlocked ? <Unlock className="w-2.5 h-2.5" /> : <Lock className="w-2.5 h-2.5" />}
                        {isUnlocked ? 'Unlocked' : 'Private'}
                      </span>
                    )}
                  </button>
                );
              })}

              {currentUser?.role === 'admin' && (
                <button
                  onClick={() => handleNavigate('admin')}
                  className={navItemClass(currentView === 'admin')}
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
                <div className="flex items-center gap-2.5 px-3 py-1.5 rounded-full bg-panel border border-border-subtle">
                  <div className="h-6 w-6 rounded-full bg-gradient-to-tr from-accent to-highlight flex items-center justify-center text-xs font-bold text-white uppercase shadow-sm">
                    {currentUser.username[0] || 'U'}
                  </div>
                  <span className="text-xs font-medium text-text-main">
                    {currentUser.username}
                  </span>
                  {currentUser.role === 'admin' && (
                    <span className="rounded bg-accent/20 px-1.5 py-0.5 text-[10px] font-semibold text-accent border border-accent/30">
                      ADMIN
                    </span>
                  )}
                </div>

                {/* Switch Profile Button */}
                <button
                  onClick={() => setIsAuthModalOpen(true)}
                  title="Switch Profile"
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium text-text-main hover:text-white bg-panel hover:bg-panel-hover border border-border-subtle transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                >
                  <Users className="h-3.5 w-3.5 text-muted" />
                  <span className="hidden sm:inline">Switch Profile</span>
                </button>

                {/* Sign Out Button */}
                <button
                  onClick={handleLogout}
                  title="Sign out"
                  className="p-2 rounded-xl text-muted hover:text-accent hover:bg-panel-hover border border-transparent hover:border-border-subtle transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
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
              <BrowseScreen
                screenId="home"
                onPlayItem={handlePlayItem}
                onNavigateToFolder={handleNavigateToFolder}
              />
            )}

            {activeLibrary && currentView === `library-${activeLibrary.id}` && (
              <BrowseScreen
                key={`screen-${activeLibrary.id}-${activeFolderNav?.libraryId === activeLibrary.id ? activeFolderNav.folderPath : 'default'}`}
                screenId={activeLibrary.id}
                onPlayItem={handlePlayItem}
                initialViewMode={activeFolderNav?.libraryId === activeLibrary.id ? 'folders' : undefined}
                initialFolder={activeFolderNav?.libraryId === activeLibrary.id ? activeFolderNav.folderPath : undefined}
                onNavigateToFolder={handleNavigateToFolder}
              />
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

            {currentView === 'admin' && currentUser?.role === 'admin' && (
              <AdminDashboard onPlayItem={handlePlayItem} onLibrariesChange={refreshLibraries} />
            )}
          </>
        )}

        {/* Switch Profile Modal (when already authenticated and user clicked Switch Profile) */}
        {currentUser && isAuthModalOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
            <div className="relative w-full max-w-2xl rounded-2xl border border-border-subtle bg-panel/95 p-6 sm:p-8 shadow-2xl backdrop-blur-md">
              <ProfileSelect
                currentUser={currentUser}
                onSuccess={handleAuthSuccess}
                onCancel={() => setIsAuthModalOpen(false)}
                onSignOut={handleLogout}
              />
            </div>
          </div>
        )}

        {/* PinKeypad Modal for Private Library Unlock */}
        {libraryToUnlock && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
            <div className="relative w-full max-w-md rounded-2xl border border-border-subtle bg-panel/95 p-6 sm:p-8 shadow-2xl backdrop-blur-md">
              <PinKeypad
                title={`Unlock ${libraryToUnlock.name}`}
                subtitle="Enter 4-digit library PIN"
                onSubmitPin={async (pin) => {
                  const res = await api.unlockLibrary(libraryToUnlock.id, pin);
                  api.storeUnlockToken(res.library_id, res.token);
                  setUnlockedLibraryIds(api.getUnlockedLibraryIds());
                  handleUnlockSuccess(libraryToUnlock);
                }}
                onSuccess={() => {}}
                onCancel={() => {
                  setLibraryToUnlock(null);
                  setPendingFolderNav(null);
                }}
              />
            </div>
          </div>
        )}
      </main>

      {/* Cinema Footer */}
      <footer className="border-t border-border-subtle py-6 text-center text-xs text-muted">
        <p>Kadr Cinema Server &copy; 2026. Built with Vite, React & Axum.</p>
      </footer>
    </div>
  );
};

export default App;
