import React, { useEffect, useState } from 'react';
import {
  FolderPlus,
  RefreshCw,
  Trash2,
  Server,
  Settings,
  Users,
  Film,
  Tv,
  CheckCircle2,
  AlertCircle,
  Plus,
  X,
  HardDrive,
  Lock,
  Folder,
} from 'lucide-react';
import { api } from '../../api/client';
import type { Library, MediaType, SystemConfig, User } from '../../types';
import { FolderPickerModal } from './FolderPickerModal';

interface AdminDashboardProps {
  onClose?: () => void;
}

export const AdminDashboard: React.FC<AdminDashboardProps> = () => {
  const [activeTab, setActiveTab] = useState<'libraries' | 'config' | 'users'>('libraries');
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [config, setConfig] = useState<SystemConfig | null>(null);
  const [users, setUsers] = useState<User[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [statusMessage, setStatusMessage] = useState<{ type: 'success' | 'error' | 'warning'; text: string } | null>(null);

  // Add Library Modal state
  const [isAddLibOpen, setIsAddLibOpen] = useState(false);
  const [libName, setLibName] = useState('');
  const [libPaths, setLibPaths] = useState<string[]>([]);
  const [isAddLibPickerOpen, setIsAddLibPickerOpen] = useState(false);
  const [cardPickerLib, setCardPickerLib] = useState<Library | null>(null);
  const [libMediaType, setLibMediaType] = useState<MediaType>('movie');

  const [isPrivate, setIsPrivate] = useState(false);
  const [libPin, setLibPin] = useState('');
  const [isSubmittingLib, setIsSubmittingLib] = useState(false);

  // Add User Modal state
  const [isAddUserOpen, setIsAddUserOpen] = useState(false);
  const [newUsername, setNewUsername] = useState('');
  const [newPin, setNewPin] = useState('');
  const [newRole, setNewRole] = useState<'admin' | 'standard'>('standard');
  const [isSubmittingUser, setIsSubmittingUser] = useState(false);

  // Config edit state
  const [editHost, setEditHost] = useState('');
  const [editPort, setEditPort] = useState(8492);
  const [editDebounce, setEditDebounce] = useState(500);
  const [editFfprobe, setEditFfprobe] = useState(true);
  const [isSavingConfig, setIsSavingConfig] = useState(false);

  // Load initial data
  const loadData = async () => {
    setIsLoading(true);
    try {
      const [libs, cfg, userList] = await Promise.all([
        api.getLibraries().catch(() => []),
        api.getSystemConfig().catch(() => null),
        api.getProfiles().catch(() => []),
      ]);
      setLibraries(libs);
      setUsers(userList);
      if (cfg) {
        setConfig(cfg);
        setEditHost(cfg.host);
        setEditPort(cfg.port);
        setEditDebounce(cfg.debounce_millis);
        setEditFfprobe(cfg.use_ffprobe);
      }
    } catch {
      setStatusMessage({ type: 'error', text: 'Failed to load administration data.' });
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const showStatus = (type: 'success' | 'error' | 'warning', text: string) => {
    setStatusMessage({ type, text });
    setTimeout(() => setStatusMessage(null), 5000);
  };

  const handleCreateLibrary = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!libName.trim() || libPaths.length === 0) return;

    if (isPrivate && libPin.length !== 4) {
      showStatus('error', 'PIN must be exactly 4 digits.');
      return;
    }

    setIsSubmittingLib(true);
    try {
      const created = await api.createLibrary({
        name: libName.trim(),
        paths: libPaths,
        media_type: libMediaType,
        is_private: isPrivate,
        pin: isPrivate ? libPin : undefined,
      });
      setLibraries((prev) => [...prev, created]);
      setIsAddLibOpen(false);
      setLibName('');
      setLibPaths([]);
      setLibMediaType('movie');
      setIsPrivate(false);
      setLibPin('');
      showStatus('success', `Library "${created.name}" created and queued for initial scan.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to create library';
      showStatus('error', msg);
    } finally {
      setIsSubmittingLib(false);
    }
  };

  const handleAddPathToLibrary = async (libraryId: string, path: string) => {
    const library = libraries.find((l) => l.id === libraryId);
    if (library) {
      const currentPaths = library.paths && library.paths.length > 0 ? library.paths : [library.path];
      if (currentPaths.includes(path)) {
        showStatus('warning', 'This folder is already part of the library');
        return;
      }
    }

    try {
      await api.addLibraryPath(libraryId, path);
      setLibraries((prev) =>
        prev.map((lib) => {
          if (lib.id === libraryId) {
            const currentPaths = lib.paths && lib.paths.length > 0 ? lib.paths : [lib.path];
            if (!currentPaths.includes(path)) {
              const updatedPaths = [...currentPaths, path];
              return { ...lib, paths: updatedPaths, path: updatedPaths[0] };
            }
          }
          return lib;
        })
      );
      showStatus('success', `Path "${path}" added to library.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to add folder path';
      showStatus('error', msg);
    }
  };

  const handleRemovePath = async (libraryId: string, path: string) => {
    try {
      await api.removeLibraryPath(libraryId, path);
      setLibraries((prev) =>
        prev.map((lib) => {
          if (lib.id === libraryId) {
            const currentPaths = lib.paths && lib.paths.length > 0 ? lib.paths : [lib.path];
            const updatedPaths = currentPaths.filter((p) => p !== path);
            return {
              ...lib,
              paths: updatedPaths,
              path: updatedPaths[0] || '',
            };
          }
          return lib;
        })
      );
      showStatus('success', `Path "${path}" removed from library.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to remove folder path';
      showStatus('error', msg);
    }
  };

  const handleDeleteLibrary = async (id: string, name: string) => {
    if (!window.confirm(`Are you sure you want to remove the library "${name}"?`)) return;

    try {
      await api.deleteLibrary(id);
      setLibraries((prev) => prev.filter((l) => l.id !== id));
      showStatus('success', `Library "${name}" was successfully removed.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to delete library';
      showStatus('error', msg);
    }
  };

  const handleScanLibrary = async (id: string, name: string) => {
    try {
      const res = await api.scanLibrary(id);
      showStatus('success', `Scan started for "${name}". Detected ${res.files_scanned} media files.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to start scan';
      showStatus('error', msg);
    }
  };

  const handleSaveConfig = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSavingConfig(true);
    try {
      const updated = await api.updateSystemConfig({
        host: editHost,
        port: Number(editPort),
        debounce_millis: Number(editDebounce),
        use_ffprobe: editFfprobe,
      });
      setConfig(updated);
      showStatus('success', 'Server configuration updated successfully.');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to update configuration';
      showStatus('error', msg);
    } finally {
      setIsSavingConfig(false);
    }
  };

  const handleCreateUser = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newUsername.trim() || newPin.length !== 4) {
      showStatus('error', 'PIN must be exactly 4 digits.');
      return;
    }

    setIsSubmittingUser(true);
    try {
      const created = await api.createUser({
        username: newUsername.trim(),
        pin: newPin,
        role: newRole,
      });
      setUsers((prev) => [...prev, created]);
      setIsAddUserOpen(false);
      setNewUsername('');
      setNewPin('');
      showStatus('success', `User profile "${created.username}" created successfully.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to create user profile';
      showStatus('error', msg);
    } finally {
      setIsSubmittingUser(false);
    }
  };

  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-8 text-text-main">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between pb-6 border-b border-border-subtle gap-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight text-text-main flex items-center gap-3">
            <Settings className="w-8 h-8 text-accent" />
            Administration & Settings
          </h1>
          <p className="text-muted mt-1">
            Manage media libraries, configure server ports and scanner pipelines, and manage users.
          </p>
        </div>

        {/* Global Feedback Banner */}
        {statusMessage && (
          <div
            className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium ${
              statusMessage.type === 'success'
                ? 'bg-emerald-950/80 border border-emerald-500/50 text-emerald-300'
                : statusMessage.type === 'warning'
                ? 'bg-amber-950/80 border border-amber-500/50 text-amber-300'
                : 'bg-cta/15 border border-cta/30 text-cta'
            }`}
          >
            {statusMessage.type === 'success' ? (
              <CheckCircle2 className="w-4 h-4 shrink-0" />
            ) : (
              <AlertCircle className="w-4 h-4 shrink-0" />
            )}
            <span>{statusMessage.text}</span>
          </div>
        )}
      </div>

      {/* Tabs Navigation */}
      <div className="flex space-x-2 mt-6 border-b border-border-subtle pb-2">
        <button
          onClick={() => setActiveTab('libraries')}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
            activeTab === 'libraries'
              ? 'bg-accent text-canvas font-semibold rounded-xl shadow-md'
              : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl'
          }`}
        >
          <Film className="w-4 h-4" />
          Libraries ({libraries.length})
        </button>
        <button
          onClick={() => setActiveTab('config')}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
            activeTab === 'config'
              ? 'bg-accent text-canvas font-semibold rounded-xl shadow-md'
              : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl'
          }`}
        >
          <Server className="w-4 h-4" />
          Server Configuration
        </button>
        <button
          onClick={() => setActiveTab('users')}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
            activeTab === 'users'
              ? 'bg-accent text-canvas font-semibold rounded-xl shadow-md'
              : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl'
          }`}
        >
          <Users className="w-4 h-4" />
          User Profiles ({users.length})
        </button>
      </div>

      {/* Tab 1: Libraries Management */}
      {activeTab === 'libraries' && (
        <div className="mt-8 space-y-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-xl font-semibold text-text-main">Configured Media Libraries</h2>
              <p className="text-muted text-sm mt-0.5">
                Directories monitored by Kadr for direct-play video files and sidecar subtitles.
              </p>
            </div>
            <button
              onClick={() => setIsAddLibOpen(true)}
              className="flex items-center gap-2 px-4 py-2 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl shadow-md text-sm transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              <Plus className="w-4 h-4" />
              Add Library
            </button>
          </div>

          {isLoading ? (
            <div className="py-12 text-center text-muted">Loading libraries...</div>
          ) : libraries.length === 0 ? (
            <div className="p-8 border border-dashed border-border-subtle bg-panel/30 rounded-xl text-center">
              <FolderPlus className="w-12 h-12 text-muted mx-auto mb-3" />
              <h3 className="text-lg font-medium text-text-main">No media libraries configured yet</h3>
              <p className="text-muted text-sm mt-1 mb-4">
                Add a local folder containing your Movies or TV Shows to start streaming.
              </p>
              <button
                onClick={() => setIsAddLibOpen(true)}
                className="inline-flex items-center gap-2 px-4 py-2 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl shadow-md text-sm transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <Plus className="w-4 h-4" />
                Add Your First Library
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {libraries.map((lib) => (
                <div
                  key={lib.id}
                  className="bg-panel border border-border-subtle rounded-xl p-5 flex flex-col justify-between hover:bg-panel-hover transition-colors shadow-sm"
                >
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <div className="flex items-center gap-2">
                        {lib.media_type === 'Movie' || lib.media_type === 'movie' ? (
                          <Film className="w-5 h-5 text-accent" />
                        ) : (
                          <Tv className="w-5 h-5 text-accent" />
                        )}
                        <h3 className="font-semibold text-lg text-text-main">{lib.name}</h3>
                      </div>

                      <div className="flex items-center gap-2">
                        {lib.is_private && (
                          <span className="flex items-center gap-1 text-xs px-2.5 py-0.5 rounded-full font-semibold bg-highlight/15 text-highlight border border-highlight/30">
                            <Lock className="w-3 h-3" />
                            Private
                          </span>
                        )}
                        <span className="text-xs px-2.5 py-0.5 rounded-full font-medium bg-canvas/60 text-muted border border-border-subtle">
                          {lib.media_type}
                        </span>
                      </div>
                    </div>
                    {/* Monitored Folder Paths */}
                    <div className="mb-4 space-y-1.5">
                      <div className="flex items-center justify-between">
                        <span className="text-[11px] uppercase tracking-wider text-muted font-semibold">
                          Monitored Paths ({((lib.paths && lib.paths.length > 0) ? lib.paths : [lib.path]).length})
                        </span>
                        <button
                          type="button"
                          onClick={() => setCardPickerLib(lib)}
                          className="flex items-center gap-1 text-xs text-accent hover:text-accent/80 font-medium cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight px-2 py-0.5 rounded-md hover:bg-accent/10 transition-colors"
                        >
                          <Plus className="w-3 h-3" />
                          + Add Folder
                        </button>
                      </div>

                      {((lib.paths && lib.paths.length > 0) ? lib.paths : [lib.path]).map((p) => {
                        const allPaths = (lib.paths && lib.paths.length > 0) ? lib.paths : [lib.path];
                        const isOnlyPath = allPaths.length <= 1;
                        return (
                          <div
                            key={p}
                            className="flex items-center justify-between text-xs font-mono text-muted bg-canvas/80 p-2 rounded-lg border border-border-subtle"
                          >
                            <div className="flex items-center gap-2 min-w-0 mr-2">
                              <Folder className="w-3.5 h-3.5 text-accent shrink-0" />
                              <span className="truncate text-text-main" title={p}>{p}</span>
                            </div>
                            <button
                              type="button"
                              disabled={isOnlyPath}
                              onClick={() => handleRemovePath(lib.id, p)}
                              title={isOnlyPath ? 'At least one path is required' : 'Remove path'}
                              aria-label={`Remove path ${p}`}
                              className="p-1 text-muted hover:text-cta hover:bg-cta/10 rounded transition-colors disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight shrink-0"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        );
                      })}
                    </div>
                  </div>

                  <div className="flex items-center justify-between pt-3 border-t border-border-subtle">
                    <span className="text-xs text-muted">
                      Added: {new Date(lib.created_at * 1000).toLocaleDateString()}
                    </span>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleScanLibrary(lib.id, lib.name)}
                        title="Re-scan directory for new media"
                        className="flex items-center gap-1 px-3 py-1.5 bg-canvas/80 hover:bg-canvas text-text-main border border-border-subtle rounded-lg text-xs font-medium transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                      >
                        <RefreshCw className="w-3.5 h-3.5" />
                        Scan Now
                      </button>
                      <button
                        onClick={() => handleDeleteLibrary(lib.id, lib.name)}
                        title="Remove library"
                        className="p-1.5 text-muted hover:text-cta hover:bg-cta/10 rounded-lg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Tab 2: Server Configuration */}
      {activeTab === 'config' && (
        <div className="mt-8 max-w-3xl space-y-6">
          <div>
            <h2 className="text-xl font-semibold text-text-main">Server & Pipeline Configuration</h2>
            <p className="text-muted text-sm mt-0.5">
              Customize listening ports, scanner behavior, and persistent directories.
            </p>
          </div>

          <form onSubmit={handleSaveConfig} className="bg-panel border border-border-subtle rounded-xl p-6 space-y-6 shadow-sm">
            {/* Network Port */}
            <div className="space-y-4">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider flex items-center gap-2">
                <Server className="w-4 h-4" />
                Network Binding
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-text-main mb-1">Listening Host</label>
                  <input
                    type="text"
                    value={editHost}
                    onChange={(e) => setEditHost(e.target.value)}
                    placeholder="0.0.0.0"
                    className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                  />
                  <p className="text-xs text-muted mt-1">Bind address (0.0.0.0 listens on all interfaces).</p>
                </div>

                <div>
                  <label className="block text-xs font-medium text-text-main mb-1">Default Port</label>
                  <input
                    type="number"
                    min="1024"
                    max="65535"
                    value={editPort}
                    onChange={(e) => setEditPort(Number(e.target.value))}
                    className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main font-mono placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                  />
                  <p className="text-xs text-muted mt-1">Default is 8492 (TCP port range 1024–65535).</p>
                </div>
              </div>
            </div>

            {/* Storage paths */}
            <div className="space-y-4 pt-4 border-t border-border-subtle">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider flex items-center gap-2">
                <HardDrive className="w-4 h-4" />
                Storage & Data Engine
              </h3>
              <div className="space-y-3 text-xs">
                <div>
                  <span className="text-muted">Database File:</span>
                  <div className="font-mono text-text-main mt-0.5 bg-canvas p-2.5 rounded-lg border border-border-subtle">
                    {config?.database_path || '/var/lib/kadr/kadr.db'}
                  </div>
                </div>
                <div>
                  <span className="text-muted">Persistent Data Directory:</span>
                  <div className="font-mono text-text-main mt-0.5 bg-canvas p-2.5 rounded-lg border border-border-subtle">
                    {config?.data_dir || '/var/lib/kadr'}
                  </div>
                </div>
                <div>
                  <span className="text-muted">Web Client Static Assets Directory:</span>
                  <div className="font-mono text-text-main mt-0.5 bg-canvas p-2.5 rounded-lg border border-border-subtle">
                    {config?.web_dir || '/usr/share/kadr/web'}
                  </div>
                </div>
              </div>
            </div>

            {/* Ingestion & Scanner */}
            <div className="space-y-4 pt-4 border-t border-border-subtle">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider flex items-center gap-2">
                <RefreshCw className="w-4 h-4" />
                Media Scanner & Metadata
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-text-main mb-1">Debounce Delay (ms)</label>
                  <input
                    type="number"
                    min="100"
                    max="5000"
                    value={editDebounce}
                    onChange={(e) => setEditDebounce(Number(e.target.value))}
                    className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main font-mono placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                  />
                  <p className="text-xs text-muted mt-1">Inotify filesystem change stabilization window.</p>
                </div>

                <div className="flex flex-col justify-center">
                  <label className="flex items-center gap-3 cursor-pointer mt-2">
                    <input
                      type="checkbox"
                      checked={editFfprobe}
                      onChange={(e) => setEditFfprobe(e.target.checked)}
                      className="w-4 h-4 rounded bg-canvas border-border-subtle text-accent accent-accent focus:ring-accent focus:ring-offset-panel"
                    />
                    <span className="text-sm font-medium text-text-main">Enable ffprobe Codec Extraction</span>
                  </label>
                  <p className="text-xs text-muted mt-1 pl-7">
                    Extracts exact resolution, video/audio streams, and container details.
                  </p>
                </div>
              </div>
            </div>

            {/* Save Button */}
            <div className="pt-4 border-t border-border-subtle flex justify-end">
              <button
                type="submit"
                disabled={isSavingConfig}
                className="px-5 py-2.5 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl text-sm transition-colors shadow-md disabled:opacity-50 cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                {isSavingConfig ? 'Saving...' : 'Save Configuration'}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Tab 3: User Management */}
      {activeTab === 'users' && (
        <div className="mt-8 space-y-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-xl font-semibold text-text-main">Registered User Profiles</h2>
              <p className="text-muted text-sm mt-0.5">
                Profiles can sign in via 4-digit PIN to track watch history and access media.
              </p>
            </div>
            <button
              onClick={() => setIsAddUserOpen(true)}
              className="flex items-center gap-2 px-4 py-2 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl text-sm transition-colors shadow-md cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              <Plus className="w-4 h-4" />
              Add User Profile
            </button>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">
            {users.map((u) => (
              <div
                key={u.id}
                className="bg-panel border border-border-subtle rounded-xl p-5 flex items-center justify-between hover:bg-panel-hover transition-colors shadow-sm"
              >
                <div className="flex items-center gap-3">
                  <div className="w-10 h-10 rounded-full bg-canvas border border-border-subtle flex items-center justify-center font-bold text-accent">
                    {u.username.slice(0, 2).toUpperCase()}
                  </div>
                  <div>
                    <h3 className="font-semibold text-text-main">{u.username}</h3>
                    <span className="text-xs px-2 py-0.5 rounded-full font-medium bg-canvas text-muted border border-border-subtle">
                      {u.role.toUpperCase()}
                    </span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Add Library Modal */}
      {isAddLibOpen && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-panel border border-border-subtle rounded-2xl max-w-lg w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-4 border-b border-border-subtle">
              <h3 className="text-lg font-bold text-text-main flex items-center gap-2">
                <FolderPlus className="w-5 h-5 text-accent" />
                Add Media Library
              </h3>
              <button
                onClick={() => {
                  setIsAddLibOpen(false);
                  setLibName('');
                  setLibPaths([]);
                  setIsPrivate(false);
                  setLibPin('');
                }}
                className="p-1 text-muted hover:text-text-main rounded-lg cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <form onSubmit={handleCreateLibrary} className="mt-4 space-y-4">
              <div>
                <label className="block text-xs font-medium text-text-main mb-1">Library Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Movies, TV Shows, Anime"
                  value={libName}
                  onChange={(e) => setLibName(e.target.value)}
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-text-main mb-1">Media Type</label>
                <select
                  value={libMediaType}
                  onChange={(e) => setLibMediaType(e.target.value as MediaType)}
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                >
                  <option value="movie">Movies (Feature films)</option>
                  <option value="show">TV Shows (Episodic series)</option>
                  <option value="anime">Anime (Anime series & films)</option>
                  <option value="music">Music & Concerts (Audio albums & live concerts)</option>
                  <option value="home_videos">Home Videos & Clips (Personal recordings)</option>
                  <option value="audiobook">Audiobooks & Podcasts (Spoken word content)</option>
                </select>
              </div>


              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <label className="block text-xs font-medium text-text-main">Folder Paths on Server</label>
                  <button
                    type="button"
                    onClick={() => setIsAddLibPickerOpen(true)}
                    className="flex items-center gap-1 text-xs text-accent hover:text-accent/80 font-medium cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight px-2 py-1 rounded-lg bg-accent/10 border border-accent/20"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    + Add Server Folder
                  </button>
                </div>

                {libPaths.length === 0 ? (
                  <div className="p-4 border border-dashed border-border-subtle bg-canvas/40 rounded-xl text-center">
                    <Folder className="w-6 h-6 text-muted mx-auto mb-1.5" />
                    <p className="text-xs text-muted">
                      No folders selected yet. Click "+ Add Server Folder" to browse.
                    </p>
                  </div>
                ) : (
                  <div className="space-y-2">
                    {libPaths.map((p) => (
                      <div
                        key={p}
                        className="flex items-center justify-between px-3 py-2 bg-canvas/80 border border-border-subtle rounded-xl text-xs font-mono text-text-main"
                      >
                        <div className="flex items-center gap-2 min-w-0 mr-2">
                          <Folder className="w-3.5 h-3.5 text-accent shrink-0" />
                          <span className="truncate" title={p}>{p}</span>
                        </div>
                        <button
                          type="button"
                          aria-label={`Remove folder ${p}`}
                          onClick={() => setLibPaths((prev) => prev.filter((item) => item !== p))}
                          className="p-1 text-muted hover:text-cta hover:bg-cta/10 rounded-lg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight shrink-0"
                        >
                          <X className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
                <p className="text-xs text-muted mt-1.5">
                  Select one or more server directories where video files are located.
                </p>
              </div>

              <div className="pt-1">
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="private-lib-toggle"
                    checked={isPrivate}
                    onChange={(e) => setIsPrivate(e.target.checked)}
                    className="w-4 h-4 rounded border-border-subtle bg-canvas text-accent accent-accent focus:ring-accent focus:ring-offset-panel cursor-pointer"
                  />
                  <label
                    htmlFor="private-lib-toggle"
                    className="text-xs font-medium text-text-main cursor-pointer flex items-center gap-1.5"
                  >
                    <Lock className="w-3.5 h-3.5 text-highlight" />
                    Private Library
                  </label>
                </div>
                <p className="text-xs text-muted mt-1">
                  Private libraries require a 4-digit PIN to access and are isolated from normal browsing.
                </p>
              </div>

              {isPrivate && (
                <div>
                  <label htmlFor="lib-pin-input" className="block text-xs font-medium text-text-main mb-1">
                    4-digit Numeric PIN
                  </label>
                  <input
                    id="lib-pin-input"
                    type="password"
                    maxLength={4}
                    inputMode="numeric"
                    required={isPrivate}
                    placeholder="4-digit PIN (e.g. 1234)"
                    value={libPin}
                    onChange={(e) => {
                      const val = e.target.value.replace(/\D/g, '').slice(0, 4);
                      setLibPin(val);
                    }}
                    className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm font-mono text-text-main tracking-widest placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                  />
                  <p className="text-xs text-muted mt-1">
                    Enter exactly 4 digits. Required to unlock this library's contents.
                  </p>
                </div>
              )}

              <div className="pt-4 border-t border-border-subtle flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => {
                    setIsAddLibOpen(false);
                    setLibName('');
                    setLibPaths([]);
                    setIsPrivate(false);
                    setLibPin('');
                  }}
                  className="px-4 py-2 bg-panel-hover hover:bg-canvas text-text-main border border-border-subtle rounded-xl text-sm font-medium transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSubmittingLib || libPaths.length === 0}
                  className="px-4 py-2 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl text-sm transition-colors shadow-md disabled:opacity-50 cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  {isSubmittingLib ? 'Creating...' : 'Create Library'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Add User Profile Modal */}
      {isAddUserOpen && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-panel border border-border-subtle rounded-2xl max-w-md w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-4 border-b border-border-subtle">
              <h3 className="text-lg font-bold text-text-main flex items-center gap-2">
                <Users className="w-5 h-5 text-accent" />
                Add User Profile
              </h3>
              <button
                onClick={() => setIsAddUserOpen(false)}
                className="p-1 text-muted hover:text-text-main rounded-lg cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <form onSubmit={handleCreateUser} className="mt-4 space-y-4">
              <div>
                <label className="block text-xs font-medium text-text-main mb-1">Profile Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. John, Kids"
                  value={newUsername}
                  onChange={(e) => setNewUsername(e.target.value)}
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-text-main mb-1">Role</label>
                <select
                  value={newRole}
                  onChange={(e) => setNewRole(e.target.value as 'admin' | 'standard')}
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                >
                  <option value="standard">Standard User</option>
                  <option value="admin">Administrator</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-medium text-text-main mb-1">4-Digit Security PIN</label>
                <input
                  type="password"
                  maxLength={4}
                  required
                  placeholder="••••"
                  value={newPin}
                  onChange={(e) => setNewPin(e.target.value.replace(/\D/g, '').slice(0, 4))}
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm font-mono text-center tracking-widest text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                />
                <p className="text-xs text-muted mt-1">Must be exactly 4 numeric digits.</p>
              </div>

              <div className="pt-4 border-t border-border-subtle flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => setIsAddUserOpen(false)}
                  className="px-4 py-2 bg-panel-hover hover:bg-canvas text-text-main border border-border-subtle rounded-xl text-sm font-medium transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSubmittingUser}
                  className="px-4 py-2 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl text-sm transition-colors shadow-md disabled:opacity-50 cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  {isSubmittingUser ? 'Creating...' : 'Create Profile'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Folder Picker for Add Library Modal */}
      <FolderPickerModal
        isOpen={isAddLibPickerOpen}
        onClose={() => setIsAddLibPickerOpen(false)}
        onSelect={(selectedPath) => {
          if (!libPaths.includes(selectedPath)) {
            setLibPaths((prev) => [...prev, selectedPath]);
          }
        }}
      />

      {/* Folder Picker for Existing Library Card */}
      <FolderPickerModal
        isOpen={!!cardPickerLib}
        initialPath={cardPickerLib?.path}
        onClose={() => setCardPickerLib(null)}
        onSelect={(selectedPath) => {
          if (cardPickerLib) {
            handleAddPathToLibrary(cardPickerLib.id, selectedPath);
          }
        }}
      />
    </div>
  );
};
