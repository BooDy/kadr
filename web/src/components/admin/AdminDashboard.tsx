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
} from 'lucide-react';
import { api } from '../../api/client';
import type { Library, SystemConfig, User } from '../../types';

interface AdminDashboardProps {
  onClose?: () => void;
}

export const AdminDashboard: React.FC<AdminDashboardProps> = () => {
  const [activeTab, setActiveTab] = useState<'libraries' | 'config' | 'users'>('libraries');
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [config, setConfig] = useState<SystemConfig | null>(null);
  const [users, setUsers] = useState<User[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [statusMessage, setStatusMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Add Library Modal state
  const [isAddLibOpen, setIsAddLibOpen] = useState(false);
  const [libName, setLibName] = useState('');
  const [libPath, setLibPath] = useState('');
  const [libMediaType, setLibMediaType] = useState<'Movie' | 'Episode'>('Movie');
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

  const showStatus = (type: 'success' | 'error', text: string) => {
    setStatusMessage({ type, text });
    setTimeout(() => setStatusMessage(null), 5000);
  };

  const handleCreateLibrary = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!libName.trim() || !libPath.trim()) return;

    setIsSubmittingLib(true);
    try {
      const created = await api.createLibrary({
        name: libName.trim(),
        path: libPath.trim(),
        media_type: libMediaType,
      });
      setLibraries((prev) => [...prev, created]);
      setIsAddLibOpen(false);
      setLibName('');
      setLibPath('');
      showStatus('success', `Library "${created.name}" created and queued for initial scan.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to create library';
      showStatus('error', msg);
    } finally {
      setIsSubmittingLib(false);
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
    <div className="w-full max-w-6xl mx-auto px-4 py-8 text-zinc-100">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between pb-6 border-b border-zinc-800 gap-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight text-white flex items-center gap-3">
            <Settings className="w-8 h-8 text-amber-500" />
            Administration & Settings
          </h1>
          <p className="text-zinc-400 mt-1">
            Manage media libraries, configure server ports and scanner pipelines, and manage users.
          </p>
        </div>

        {/* Global Feedback Banner */}
        {statusMessage && (
          <div
            className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium ${
              statusMessage.type === 'success'
                ? 'bg-emerald-950/80 border border-emerald-500/50 text-emerald-300'
                : 'bg-rose-950/80 border border-rose-500/50 text-rose-300'
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
      <div className="flex space-x-2 mt-6 border-b border-zinc-800 pb-2">
        <button
          onClick={() => setActiveTab('libraries')}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium transition-colors ${
            activeTab === 'libraries'
              ? 'bg-amber-500 text-zinc-950 font-semibold shadow-md'
              : 'text-zinc-400 hover:text-white hover:bg-zinc-900'
          }`}
        >
          <Film className="w-4 h-4" />
          Libraries ({libraries.length})
        </button>
        <button
          onClick={() => setActiveTab('config')}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium transition-colors ${
            activeTab === 'config'
              ? 'bg-amber-500 text-zinc-950 font-semibold shadow-md'
              : 'text-zinc-400 hover:text-white hover:bg-zinc-900'
          }`}
        >
          <Server className="w-4 h-4" />
          Server Configuration
        </button>
        <button
          onClick={() => setActiveTab('users')}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium transition-colors ${
            activeTab === 'users'
              ? 'bg-amber-500 text-zinc-950 font-semibold shadow-md'
              : 'text-zinc-400 hover:text-white hover:bg-zinc-900'
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
              <h2 className="text-xl font-semibold text-white">Configured Media Libraries</h2>
              <p className="text-zinc-400 text-sm mt-0.5">
                Directories monitored by Kadr for direct-play video files and sidecar subtitles.
              </p>
            </div>
            <button
              onClick={() => setIsAddLibOpen(true)}
              className="flex items-center gap-2 px-4 py-2 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-semibold rounded-lg text-sm transition-colors shadow"
            >
              <Plus className="w-4 h-4" />
              Add Library
            </button>
          </div>

          {isLoading ? (
            <div className="py-12 text-center text-zinc-500">Loading libraries...</div>
          ) : libraries.length === 0 ? (
            <div className="p-8 border border-dashed border-zinc-800 rounded-xl text-center">
              <FolderPlus className="w-12 h-12 text-zinc-600 mx-auto mb-3" />
              <h3 className="text-lg font-medium text-zinc-300">No media libraries configured yet</h3>
              <p className="text-zinc-500 text-sm mt-1 mb-4">
                Add a local folder containing your Movies or TV Shows to start streaming.
              </p>
              <button
                onClick={() => setIsAddLibOpen(true)}
                className="inline-flex items-center gap-2 px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-white rounded-lg text-sm font-medium transition-colors"
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
                  className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 flex flex-col justify-between hover:border-zinc-700 transition-colors shadow-sm"
                >
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <div className="flex items-center gap-2">
                        {lib.media_type === 'Movie' ? (
                          <Film className="w-5 h-5 text-amber-500" />
                        ) : (
                          <Tv className="w-5 h-5 text-sky-400" />
                        )}
                        <h3 className="font-semibold text-lg text-white">{lib.name}</h3>
                      </div>
                      <span className="text-xs px-2.5 py-0.5 rounded-full font-medium bg-zinc-800 text-zinc-300 border border-zinc-700">
                        {lib.media_type}
                      </span>
                    </div>
                    <div className="text-xs font-mono text-zinc-400 break-all bg-zinc-950/60 p-2.5 rounded-lg border border-zinc-800/80 mb-4">
                      {lib.path}
                    </div>
                  </div>

                  <div className="flex items-center justify-between pt-3 border-t border-zinc-800/60">
                    <span className="text-xs text-zinc-500">
                      Added: {new Date(lib.created_at * 1000).toLocaleDateString()}
                    </span>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleScanLibrary(lib.id, lib.name)}
                        title="Re-scan directory for new media"
                        className="flex items-center gap-1 px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-xs font-medium transition-colors"
                      >
                        <RefreshCw className="w-3.5 h-3.5" />
                        Scan Now
                      </button>
                      <button
                        onClick={() => handleDeleteLibrary(lib.id, lib.name)}
                        title="Remove library"
                        className="p-1.5 text-zinc-400 hover:text-rose-400 hover:bg-rose-950/40 rounded-lg transition-colors"
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
            <h2 className="text-xl font-semibold text-white">Server & Pipeline Configuration</h2>
            <p className="text-zinc-400 text-sm mt-0.5">
              Customize listening ports, scanner behavior, and persistent directories.
            </p>
          </div>

          <form onSubmit={handleSaveConfig} className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-6">
            {/* Network Port */}
            <div className="space-y-4">
              <h3 className="text-sm font-semibold text-amber-400 uppercase tracking-wider flex items-center gap-2">
                <Server className="w-4 h-4" />
                Network Binding
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-zinc-300 mb-1">Listening Host</label>
                  <input
                    type="text"
                    value={editHost}
                    onChange={(e) => setEditHost(e.target.value)}
                    placeholder="0.0.0.0"
                    className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-amber-500"
                  />
                  <p className="text-xs text-zinc-500 mt-1">Bind address (0.0.0.0 listens on all interfaces).</p>
                </div>

                <div>
                  <label className="block text-xs font-medium text-zinc-300 mb-1">Default Port</label>
                  <input
                    type="number"
                    min="1024"
                    max="65535"
                    value={editPort}
                    onChange={(e) => setEditPort(Number(e.target.value))}
                    className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white font-mono focus:outline-none focus:border-amber-500"
                  />
                  <p className="text-xs text-zinc-500 mt-1">Default is 8492 (TCP port range 1024–65535).</p>
                </div>
              </div>
            </div>

            {/* Storage paths */}
            <div className="space-y-4 pt-4 border-t border-zinc-800">
              <h3 className="text-sm font-semibold text-amber-400 uppercase tracking-wider flex items-center gap-2">
                <HardDrive className="w-4 h-4" />
                Storage & Data Engine
              </h3>
              <div className="space-y-3 text-xs">
                <div>
                  <span className="text-zinc-500">Database File:</span>
                  <div className="font-mono text-zinc-300 mt-0.5 bg-zinc-950 p-2 rounded border border-zinc-800">
                    {config?.database_path || '/var/lib/kadr/kadr.db'}
                  </div>
                </div>
                <div>
                  <span className="text-zinc-500">Persistent Data Directory:</span>
                  <div className="font-mono text-zinc-300 mt-0.5 bg-zinc-950 p-2 rounded border border-zinc-800">
                    {config?.data_dir || '/var/lib/kadr'}
                  </div>
                </div>
                <div>
                  <span className="text-zinc-500">Web Client Static Assets Directory:</span>
                  <div className="font-mono text-zinc-300 mt-0.5 bg-zinc-950 p-2 rounded border border-zinc-800">
                    {config?.web_dir || '/usr/share/kadr/web'}
                  </div>
                </div>
              </div>
            </div>

            {/* Ingestion & Scanner */}
            <div className="space-y-4 pt-4 border-t border-zinc-800">
              <h3 className="text-sm font-semibold text-amber-400 uppercase tracking-wider flex items-center gap-2">
                <RefreshCw className="w-4 h-4" />
                Media Scanner & Metadata
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-zinc-300 mb-1">Debounce Delay (ms)</label>
                  <input
                    type="number"
                    min="100"
                    max="5000"
                    value={editDebounce}
                    onChange={(e) => setEditDebounce(Number(e.target.value))}
                    className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white font-mono focus:outline-none focus:border-amber-500"
                  />
                  <p className="text-xs text-zinc-500 mt-1">Inotify filesystem change stabilization window.</p>
                </div>

                <div className="flex flex-col justify-center">
                  <label className="flex items-center gap-3 cursor-pointer mt-2">
                    <input
                      type="checkbox"
                      checked={editFfprobe}
                      onChange={(e) => setEditFfprobe(e.target.checked)}
                      className="w-4 h-4 rounded bg-zinc-950 border-zinc-800 text-amber-500 focus:ring-amber-500 focus:ring-offset-zinc-900"
                    />
                    <span className="text-sm font-medium text-zinc-200">Enable ffprobe Codec Extraction</span>
                  </label>
                  <p className="text-xs text-zinc-500 mt-1 pl-7">
                    Extracts exact resolution, video/audio streams, and container details.
                  </p>
                </div>
              </div>
            </div>

            {/* Save Button */}
            <div className="pt-4 border-t border-zinc-800 flex justify-end">
              <button
                type="submit"
                disabled={isSavingConfig}
                className="px-5 py-2.5 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-semibold rounded-lg text-sm transition-colors shadow disabled:opacity-50"
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
              <h2 className="text-xl font-semibold text-white">Registered User Profiles</h2>
              <p className="text-zinc-400 text-sm mt-0.5">
                Profiles can sign in via 4-digit PIN to track watch history and access media.
              </p>
            </div>
            <button
              onClick={() => setIsAddUserOpen(true)}
              className="flex items-center gap-2 px-4 py-2 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-semibold rounded-lg text-sm transition-colors shadow"
            >
              <Plus className="w-4 h-4" />
              Add User Profile
            </button>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">
            {users.map((u) => (
              <div
                key={u.id}
                className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 flex items-center justify-between"
              >
                <div className="flex items-center gap-3">
                  <div className="w-10 h-10 rounded-full bg-zinc-800 border border-zinc-700 flex items-center justify-center font-bold text-amber-400">
                    {u.username.slice(0, 2).toUpperCase()}
                  </div>
                  <div>
                    <h3 className="font-semibold text-white">{u.username}</h3>
                    <span className="text-xs px-2 py-0.5 rounded-full font-medium bg-zinc-800 text-zinc-400 border border-zinc-700/50">
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
          <div className="bg-zinc-900 border border-zinc-800 rounded-2xl max-w-lg w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-4 border-b border-zinc-800">
              <h3 className="text-lg font-bold text-white flex items-center gap-2">
                <FolderPlus className="w-5 h-5 text-amber-500" />
                Add Media Library
              </h3>
              <button
                onClick={() => setIsAddLibOpen(false)}
                className="p-1 text-zinc-400 hover:text-white rounded-lg"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <form onSubmit={handleCreateLibrary} className="mt-4 space-y-4">
              <div>
                <label className="block text-xs font-medium text-zinc-300 mb-1">Library Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Movies, TV Shows, Anime"
                  value={libName}
                  onChange={(e) => setLibName(e.target.value)}
                  className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-amber-500"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-zinc-300 mb-1">Media Type</label>
                <select
                  value={libMediaType}
                  onChange={(e) => setLibMediaType(e.target.value as 'Movie' | 'Episode')}
                  className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-amber-500"
                >
                  <option value="Movie">Movies</option>
                  <option value="Episode">TV Shows / Episodes</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-medium text-zinc-300 mb-1">Folder Path on Server</label>
                <input
                  type="text"
                  required
                  placeholder="/var/lib/kadr/media/movies"
                  value={libPath}
                  onChange={(e) => setLibPath(e.target.value)}
                  className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm font-mono text-white focus:outline-none focus:border-amber-500"
                />
                <p className="text-xs text-zinc-500 mt-1">
                  Absolute path on the host where video files (.mkv, .mp4, .avi) are stored.
                </p>
              </div>

              <div className="pt-4 border-t border-zinc-800 flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => setIsAddLibOpen(false)}
                  className="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-sm font-medium transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSubmittingLib}
                  className="px-4 py-2 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-semibold rounded-lg text-sm transition-colors disabled:opacity-50"
                >
                  {isSubmittingLib ? 'Creating...' : 'Create Library'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Add User Modal */}
      {isAddUserOpen && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-zinc-900 border border-zinc-800 rounded-2xl max-w-md w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-4 border-b border-zinc-800">
              <h3 className="text-lg font-bold text-white flex items-center gap-2">
                <Users className="w-5 h-5 text-amber-500" />
                Add User Profile
              </h3>
              <button
                onClick={() => setIsAddUserOpen(false)}
                className="p-1 text-zinc-400 hover:text-white rounded-lg"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <form onSubmit={handleCreateUser} className="mt-4 space-y-4">
              <div>
                <label className="block text-xs font-medium text-zinc-300 mb-1">Profile Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. John, Kids"
                  value={newUsername}
                  onChange={(e) => setNewUsername(e.target.value)}
                  className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-amber-500"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-zinc-300 mb-1">Role</label>
                <select
                  value={newRole}
                  onChange={(e) => setNewRole(e.target.value as 'admin' | 'standard')}
                  className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-amber-500"
                >
                  <option value="standard">Standard User</option>
                  <option value="admin">Administrator</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-medium text-zinc-300 mb-1">4-Digit Security PIN</label>
                <input
                  type="password"
                  maxLength={4}
                  required
                  placeholder="••••"
                  value={newPin}
                  onChange={(e) => setNewPin(e.target.value.replace(/\D/g, '').slice(0, 4))}
                  className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm font-mono text-center tracking-widest text-white focus:outline-none focus:border-amber-500"
                />
                <p className="text-xs text-zinc-500 mt-1">Must be exactly 4 numeric digits.</p>
              </div>

              <div className="pt-4 border-t border-zinc-800 flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => setIsAddUserOpen(false)}
                  className="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-sm font-medium transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSubmittingUser}
                  className="px-4 py-2 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-semibold rounded-lg text-sm transition-colors disabled:opacity-50"
                >
                  {isSubmittingUser ? 'Creating...' : 'Create Profile'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
