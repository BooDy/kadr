import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { ApiClient, ApiError, describeApiError } from './client';
import type { User, DownloadSubtitleRequest } from '../types';

describe('ApiClient', () => {
  let client: ApiClient;
  let mockFetch: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    localStorage.clear();
    mockFetch = vi.fn();
    globalThis.fetch = mockFetch;
    client = new ApiClient();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Token & User Management', () => {
    it('initializes with null token and user if localStorage is empty', () => {
      expect(client.getToken()).toBeNull();
      expect(client.getUser()).toBeNull();
    });

    it('stores and retrieves token from localStorage', () => {
      client.setToken('test-token-123');
      expect(client.getToken()).toBe('test-token-123');
      expect(localStorage.getItem('kadr_token')).toBe('test-token-123');

      client.setToken(null);
      expect(client.getToken()).toBeNull();
      expect(localStorage.getItem('kadr_token')).toBeNull();
    });

    it('stores and retrieves user from localStorage', () => {
      const user: User = { id: 'admin-1', username: 'admin', role: 'admin' };
      client.setUser(user);
      expect(client.getUser()).toEqual(user);
      expect(JSON.parse(localStorage.getItem('kadr_user')!)).toEqual(user);

      client.setUser(null);
      expect(client.getUser()).toBeNull();
      expect(localStorage.getItem('kadr_user')).toBeNull();
    });
  });

  describe('Auth Header Injection', () => {
    it('injects Authorization: Bearer <token> when token is present', async () => {
      client.setToken('jwt-abc-123');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => [{ id: 'home', title: 'Home' }],
      });

      const screens = await client.getScreens();
      expect(screens).toEqual([{ id: 'home', title: 'Home' }]);

      expect(mockFetch).toHaveBeenCalledTimes(1);
      const [url, options] = mockFetch.mock.calls[0];
      expect(url).toBe('/api/v1/screens');
      expect(options.headers).toMatchObject({
        Authorization: 'Bearer jwt-abc-123',
      });
    });

    it('does not include Authorization header when token is null', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ token: 'new-token', user_id: 'admin-1', username: 'admin', role: 'admin' }),
      });

      await client.loginWithPin('1234');
      const [url, options] = mockFetch.mock.calls[0];
      expect(url).toBe('/api/v1/auth/pin');
      expect(options.headers?.Authorization).toBeUndefined();
    });
  });

  describe('Error Handling', () => {
    it('throws ApiError with status and message on HTTP failure', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 401,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ error: 'Invalid user or PIN' }),
      });

      let caught: any;
      try {
        await client.loginWithPin('0000');
      } catch (err: any) {
        caught = err;
      }

      expect(caught).toBeInstanceOf(ApiError);
      expect(caught.status).toBe(401);
      expect(caught.message).toBe('Invalid user or PIN');
    });

    it('handles non-JSON error response gracefully', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 502,
        headers: new Headers({ 'content-type': 'text/plain' }),
        text: async () => 'Bad Gateway',
        json: async () => {
          throw new Error('Not JSON');
        },
      });

      await expect(client.getScreens()).rejects.toThrow(ApiError);
    });

    it('clears token and user on 401 unauthorized response', async () => {
      client.setToken('expired-token');
      client.setUser({ id: 'u1', username: 'u', role: 'standard' });

      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 401,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ error: 'Unauthorized' }),
      });

      await expect(client.getScreens()).rejects.toThrow(ApiError);
      expect(client.getToken()).toBeNull();
      expect(client.getUser()).toBeNull();
    });
  });

  describe('URL Resolution & Methods', () => {
    it('resolves stream URL correctly', () => {
      expect(client.getStreamUrl(42)).toBe('/api/v1/stream/42');
    });

    it('resolves stream URL with token when token is present', () => {
      client.setToken('test-token-123');
      expect(client.getStreamUrl(42)).toBe('/api/v1/stream/42?token=test-token-123');
    });

    it('resolves subtitle stream URL correctly', () => {
      expect(client.getSubtitleStreamUrl(99)).toBe('/api/v1/subtitles/99/stream.vtt');
      client.setToken('test-token-123');
      expect(client.getSubtitleStreamUrl(99)).toBe('/api/v1/subtitles/99/stream.vtt?token=test-token-123');
    });

    it('resolves artwork URLs correctly', () => {
      expect(client.getArtworkUrl(42, 'poster')).toBe('/api/v1/artwork/42/poster');
      expect(client.getArtworkUrl(42, 'backdrop')).toBe('/api/v1/artwork/42/backdrop');
    });

    it('calls getScreen with proper URL and deserializes response', async () => {
      client.setToken('tok');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ id: 'home', title: 'Home', widgets: [] }),
      });

      const layout = await client.getScreen('home');
      expect(layout.id).toBe('home');
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/screens/home');
    });

    it('calls saveScreen with PUT and serializes layout', async () => {
      client.setToken('tok');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ id: 'home', title: 'Home', widgets: [] }),
      });

      const layout = await client.saveScreen('home', { id: 'home', title: 'Home', widgets: [] });
      expect(layout.id).toBe('home');
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/screens/home');
      expect(mockFetch.mock.calls[0][1]?.method).toBe('PUT');
      expect(JSON.parse(mockFetch.mock.calls[0][1]?.body as string)).toEqual({
        id: 'home',
        title: 'Home',
        widgets: [],
      });
    });

    it('calls createScreen with POST and payload', async () => {
      client.setToken('tok');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ id: 'custom_1', title: 'Custom', widgets: [] }),
      });

      const res = await client.createScreen({ id: 'custom_1', title: 'Custom', description: 'desc' });
      expect(res.id).toBe('custom_1');
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/screens');
      expect(mockFetch.mock.calls[0][1]?.method).toBe('POST');
      expect(JSON.parse(mockFetch.mock.calls[0][1]?.body as string)).toEqual({
        id: 'custom_1',
        title: 'Custom',
        description: 'desc',
      });
    });

    it('calls resetScreen with DELETE', async () => {
      client.setToken('tok');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ message: 'Screen reset to defaults' }),
      });

      const res = await client.resetScreen('home');
      expect(res).toBeDefined();
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/screens/home');
      expect(mockFetch.mock.calls[0][1]?.method).toBe('DELETE');
    });

    it('calls deleteScreen with DELETE', async () => {
      client.setToken('tok');
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ message: 'Custom screen deleted' }),
      });

      await client.deleteScreen('custom_1');
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/screens/custom_1');
      expect(mockFetch.mock.calls[0][1]?.method).toBe('DELETE');
    });

    it('calls getWidgetData and extracts items', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          widget_id: 'trending',
          items: [{ id: 1, title: 'Inception', media_type: 'movie' }],
        }),
      });

      const items = await client.getWidgetData('trending', 0, 10);
      expect(items).toEqual([{ id: 1, title: 'Inception', media_type: 'movie' }]);
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/widgets/trending/data?offset=0&limit=10');
    });

    it('calls getItemDetails and returns details payload', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          card: { id: 10, title: 'Interstellar', media_type: 'movie' },
          overview: 'Space voyage',
          genres: ['Sci-Fi'],
          stream_url: '/api/v1/stream/10',
        }),
      });

      const details = await client.getItemDetails(10);
      expect(details.card.title).toBe('Interstellar');
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/items/10/details');
    });

    it('calls subtitle methods: getSubtitles, searchSubtitles, downloadSubtitle, deleteSubtitle', async () => {
      // getSubtitles
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => [
          {
            id: 1,
            media_item_id: 10,
            source: 'sidecar',
            language: 'eng',
            format: 'srt',
            is_default: true,
            is_forced: false,
            stream_url: '/api/v1/subtitles/1/stream.vtt',
          },
        ],
      });
      const subs = await client.getSubtitles(10);
      expect(subs).toHaveLength(1);
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/items/10/subtitles');

      // searchSubtitles
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          configured: true,
          matches: [
            {
              id: 'sub-1',
              language: 'en',
              hearing_impaired: false,
              format: 'srt',
              download_count: 500,
            },
          ],
        }),
      });
      const searchRes = await client.searchSubtitles(10, 'en,ar');
      expect(searchRes.configured).toBe(true);
      expect(searchRes.matches).toHaveLength(1);
      expect(mockFetch.mock.calls[1][0]).toBe('/api/v1/subtitles/10/search?languages=en%2Car');

      // downloadSubtitle
      const dlReq: DownloadSubtitleRequest = {
        file_id: 'sub-1',
        language: 'en',
        title: 'English',
        is_forced: false,
      };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 201,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          id: 2,
          media_item_id: 10,
          source: 'downloaded',
          language: 'en',
          format: 'srt',
          is_default: false,
          is_forced: false,
          stream_url: '/api/v1/subtitles/2/stream.vtt',
        }),
      });
      const dlRes = await client.downloadSubtitle(10, dlReq);
      expect(dlRes.id).toBe(2);
      expect(mockFetch.mock.calls[2][0]).toBe('/api/v1/subtitles/10/download');
      expect(JSON.parse(mockFetch.mock.calls[2][1].body)).toEqual(dlReq);

      // deleteSubtitle
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({ deleted: true }),
      });
      const delRes = await client.deleteSubtitle(2);
      expect(delRes).toEqual({ deleted: true });
      expect(mockFetch.mock.calls[3][0]).toBe('/api/v1/subtitles/2');
      expect(mockFetch.mock.calls[3][1].method).toBe('DELETE');
    });

    it('calls getTelemetry and returns snapshot', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          active_sessions_count: 3,
          rss_memory_bytes: 45000000,
          db_size_bytes: 120000,
          wal_size_bytes: 32000,
          timestamp: 1700000000,
        }),
      });

      const telem = await client.getTelemetry();
      expect(telem.active_sessions_count).toBe(3);
      expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/system/telemetry');
    });

    it('safely handles empty response bodies on 200 or 204 responses', async () => {
      // 204 No Content
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 204,
        headers: new Headers(),
      });
      const res204 = await client.request('/api/v1/empty-204');
      expect(res204).toBeUndefined();

      // 200 with empty text body
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        text: async () => '',
      });
      const res200Empty = await client.request('/api/v1/empty-200');
      expect(res200Empty).toBeUndefined();
    });
  });

  describe('Unlocked Libraries Management', () => {
    beforeEach(() => {
      sessionStorage.clear();
    });

    it('stores, retrieves, and clears unlock tokens in sessionStorage', () => {
      expect(client.getUnlockedTokens()).toEqual({});
      expect(client.getUnlockedLibraryIds()).toEqual([]);
      expect(client.isLibraryUnlocked('lib-1')).toBe(false);

      client.storeUnlockToken('lib-1', 'token-1');
      expect(client.getUnlockedTokens()).toEqual({ 'lib-1': 'token-1' });
      expect(client.getUnlockedLibraryIds()).toEqual(['lib-1']);
      expect(client.isLibraryUnlocked('lib-1')).toBe(true);
      expect(client.getUnlockToken('lib-1')).toBe('token-1');

      client.storeUnlockToken('lib-2', 'token-2');
      expect(client.getUnlockedLibraryIds()).toEqual(['lib-1', 'lib-2']);

      client.clearUnlockedToken('lib-1');
      expect(client.isLibraryUnlocked('lib-1')).toBe(false);
      expect(client.isLibraryUnlocked('lib-2')).toBe(true);

      client.clearUnlockedTokens();
      expect(client.getUnlockedTokens()).toEqual({});
      expect(client.getUnlockedLibraryIds()).toEqual([]);
    });

    it('injects X-Kadr-Unlocked header into outgoing requests when unlocked tokens exist', async () => {
      client.storeUnlockToken('lib-1', 'token-abc');
      client.storeUnlockToken('lib-2', 'token-def');

      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => [],
      });

      await client.getLibraries();

      expect(mockFetch).toHaveBeenCalledTimes(1);
      const [url, options] = mockFetch.mock.calls[0];
      expect(url).toBe('/api/v1/libraries');
      expect(options.headers['X-Kadr-Unlocked']).toContain('token-abc');
      expect(options.headers['X-Kadr-Unlocked']).toContain('token-def');
    });

    it('getLibraryFolders requests folders with or without path parameter', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          library_id: 'lib-1',
          library_name: 'Movies',
          current_path: '',
          parent_path: null,
          breadcrumbs: [],
          directories: [],
          items: [],
        }),
      });

      await client.getLibraryFolders('lib-1');
      expect(mockFetch).toHaveBeenCalledWith('/api/v1/libraries/lib-1/folders', expect.anything());

      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          library_id: 'lib-1',
          library_name: 'Movies',
          current_path: 'Sci-Fi/Cyberpunk',
          parent_path: 'Sci-Fi',
          breadcrumbs: [],
          directories: [],
          items: [],
        }),
      });

      await client.getLibraryFolders('lib-1', 'Sci-Fi/Cyberpunk');
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/v1/libraries/lib-1/folders?path=Sci-Fi%2FCyberpunk',
        expect.anything()
      );
    });

    it('appends unlocked query param to getStreamUrl when tokens exist', () => {
      client.storeUnlockToken('lib-1', 'token-stream-123');
      const url = client.getStreamUrl(42);
      expect(url).toBe('/api/v1/stream/42?unlocked=token-stream-123');
    });

    it('unlockLibrary posts PIN, returns response, and saves token', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          library_id: 'lib-secret',
          token: 'signed-unlock-token-999',
          expires_at: 1700086400,
        }),
      });

      const res = await client.unlockLibrary('lib-secret', '1234');
      expect(res.library_id).toBe('lib-secret');
      expect(res.token).toBe('signed-unlock-token-999');

      const [url, options] = mockFetch.mock.calls[0];
      expect(url).toBe('/api/v1/libraries/lib-secret/unlock');
      expect(options.method).toBe('POST');
      expect(JSON.parse(options.body)).toEqual({ pin: '1234' });

      // Verified token is saved in sessionStorage
      expect(client.isLibraryUnlocked('lib-secret')).toBe(true);
      expect(client.getUnlockToken('lib-secret')).toBe('signed-unlock-token-999');
    });

    it('logout clears unlocked tokens from sessionStorage', () => {
      client.storeUnlockToken('lib-1', 'token-xyz');
      expect(client.isLibraryUnlocked('lib-1')).toBe(true);

      client.logout();
      expect(client.isLibraryUnlocked('lib-1')).toBe(false);
      expect(sessionStorage.getItem('kadr_unlocked_libraries')).toBeNull();
    });
  });

  describe('Multi-Path and Filesystem Browsing', () => {
    it('browseFilesystem requests /api/v1/system/fs with encoded path query', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          current_path: '/media/movies',
          parent_path: '/media',
          directories: [{ name: 'action', path: '/media/movies/action' }],
          shortcuts: [{ name: 'Media', path: '/media' }],
        }),
      });

      const res = await client.browseFilesystem('/media/movies');
      expect(res.current_path).toBe('/media/movies');
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/v1/system/fs?path=%2Fmedia%2Fmovies',
        expect.anything()
      );
    });

    it('browseFilesystem requests /api/v1/system/fs without query if path is omitted', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({
          current_path: '/',
          parent_path: null,
          directories: [],
          shortcuts: [],
        }),
      });

      const res = await client.browseFilesystem();
      expect(res.current_path).toBe('/');
      expect(mockFetch).toHaveBeenCalledWith('/api/v1/system/fs', expect.anything());
    });

    it('addLibraryPath posts path to /api/v1/libraries/:id/paths', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({}),
      });

      await client.addLibraryPath('lib-123', '/mnt/nas/media');
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/v1/libraries/lib-123/paths',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ path: '/mnt/nas/media' }),
        })
      );
    });

    it('removeLibraryPath sends DELETE to /api/v1/libraries/:id/paths with path query', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => ({}),
      });

      await client.removeLibraryPath('lib-123', '/mnt/nas/media');
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/v1/libraries/lib-123/paths?path=%2Fmnt%2Fnas%2Fmedia',
        expect.objectContaining({
          method: 'DELETE',
        })
      );
    });
  });

  describe('Discovery', () => {
    it('getDiscoveryInfo fetches /api/v1/discovery and returns DiscoveryResponse', async () => {
      const mockDiscovery = {
        app: 'kadr',
        server_id: '550e8400-e29b-41d4-a716-446655440000',
        name: 'Living Room Kadr',
        version: '0.1.0',
        protocol_version: 1,
        port: 8492,
        setup_completed: true,
        status: 'online',
      };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: async () => mockDiscovery,
      });

      const res = await client.getDiscoveryInfo();
      expect(res).toEqual(mockDiscovery);
      expect(mockFetch).toHaveBeenCalledWith('/api/v1/discovery', expect.anything());
    });
  });
});

describe('ApiClient plugin management', () => {
  let client: ApiClient;
  let mockFetch: ReturnType<typeof vi.fn>;

  const jsonResponse = (body: unknown, status = 200) => ({
    ok: status >= 200 && status < 300,
    status,
    statusText: 'x',
    headers: new Headers({ 'content-type': 'application/json' }),
    json: async () => body,
  });

  beforeEach(() => {
    localStorage.clear();
    mockFetch = vi.fn();
    globalThis.fetch = mockFetch;
    client = new ApiClient();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('gets and updates plugin settings', async () => {
    const settings = { enabled: false, resolve_timeout_secs: 5, allow_private_proxy: false };
    mockFetch.mockResolvedValueOnce(jsonResponse(settings));
    expect(await client.getPluginSettings()).toEqual(settings);
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins/settings');

    mockFetch.mockResolvedValueOnce(jsonResponse({ ...settings, enabled: true }));
    const updated = await client.updatePluginSettings({ enabled: true });
    expect(updated.enabled).toBe(true);
    const [url, init] = mockFetch.mock.calls[1];
    expect(url).toBe('/api/v1/admin/plugins/settings');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body)).toEqual({ enabled: true });
  });

  it('lists and registers plugins', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse([]));
    expect(await client.listPlugins()).toEqual([]);
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins');

    mockFetch.mockResolvedValueOnce(jsonResponse({ plugin: { id: 'hello' }, secret: 's3' }, 201));
    const res = await client.registerPlugin('http://127.0.0.1:9100');
    expect(res.secret).toBe('s3');
    const [url, init] = mockFetch.mock.calls[1];
    expect(url).toBe('/api/v1/admin/plugins');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ base_url: 'http://127.0.0.1:9100' });
  });

  it('approves with and without an explicit permission list', async () => {
    mockFetch.mockResolvedValue(jsonResponse({ plugin: { id: 'hello' }, token: 't' }));
    await client.approvePlugin('hello', ['items:publish']);
    let [url, init] = mockFetch.mock.calls[0];
    expect(url).toBe('/api/v1/admin/plugins/hello/approve');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ permissions: ['items:publish'] });

    await client.approvePlugin('hello');
    [url, init] = mockFetch.mock.calls[1];
    expect(init.body).toBeUndefined();
  });

  it('enables, disables and removes plugins', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ id: 'hello', status: 'disabled' }));
    await client.setPluginEnabled('hello', false);
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins/hello/disable');
    mockFetch.mockResolvedValueOnce(jsonResponse({ id: 'hello', status: 'enabled' }));
    await client.setPluginEnabled('hello', true);
    expect(mockFetch.mock.calls[1][0]).toBe('/api/v1/admin/plugins/hello/enable');

    mockFetch.mockResolvedValueOnce({ ok: true, status: 204, statusText: 'No Content', headers: new Headers() });
    await client.removePlugin('hello');
    expect(mockFetch.mock.calls[2][0]).toBe('/api/v1/admin/plugins/hello');
    expect(mockFetch.mock.calls[2][1].method).toBe('DELETE');

    mockFetch.mockResolvedValueOnce({ ok: true, status: 204, statusText: 'No Content', headers: new Headers() });
    await client.removePlugin('hello', true);
    expect(mockFetch.mock.calls[3][0]).toBe('/api/v1/admin/plugins/hello?purge=true');
  });

  it('fetches connection details and issues tokens, encoding the id', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ base_url: 'http://x', secret: 's' }));
    const conn = await client.getPluginConnection('a b');
    expect(conn.secret).toBe('s');
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins/a%20b/connection');

    mockFetch.mockResolvedValueOnce(jsonResponse({ token: 'tok' }));
    expect((await client.issuePluginToken('hello')).token).toBe('tok');
    expect(mockFetch.mock.calls[1][0]).toBe('/api/v1/admin/plugins/hello/token');
    expect(mockFetch.mock.calls[1][1].method).toBe('POST');
  });

  it('describeApiError prefers the API message over the error code', async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: 'PLUGIN_UNAVAILABLE', message: 'Plugin unavailable: connection failed' }, 503)
    );
    const err = await client.registerPlugin('http://x').catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.message).toBe('PLUGIN_UNAVAILABLE');
    expect(describeApiError(err)).toBe('Plugin unavailable: connection failed');

    expect(describeApiError(new ApiError(500, 'boom'))).toBe('boom');
    expect(describeApiError(new Error('plain'))).toBe('plain');
    expect(describeApiError('weird')).toBe('Unexpected error');
  });
});

describe('Playlists API', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    vi.restoreAllMocks();
  });

  it('fetches playlists with optional contains_item_id and forwards auth + unlock headers', async () => {
    const client = new ApiClient('http://localhost:8492');
    client.setToken('jwt-abc');
    client.storeUnlockToken('lib-priv', 'unlock-sig-1');

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(JSON.stringify({ playlists: [] }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
    );

    await client.getPlaylists();
    expect(fetchSpy).toHaveBeenLastCalledWith(
      'http://localhost:8492/api/v1/playlists',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer jwt-abc',
          'X-Kadr-Unlocked': 'unlock-sig-1',
        }),
      })
    );

    await client.getPlaylists(42);
    expect(fetchSpy).toHaveBeenLastCalledWith(
      'http://localhost:8492/api/v1/playlists?contains_item_id=42',
      expect.any(Object)
    );
  });

  it('calls create, get, update, delete, addItems, removeItem, and reorderItems endpoints', async () => {
    const client = new ApiClient('');
    client.setToken('jwt-abc');

    const mockSummary = {
      id: 'pl-1',
      name: 'Sci-Fi',
      owner_user_id: 'u-1',
      owner_username: 'alex',
      is_public: false,
      is_editable: true,
      visible_item_count: 2,
      locked_item_count: 0,
      total_duration_seconds: 7200,
      cover_poster_urls: [],
      created_at: 100,
      updated_at: 200,
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (_url, init) => {
      if (init?.method === 'DELETE' && String(_url).endsWith('/playlists/pl-1')) {
        return new Response(null, { status: 204 });
      }
      if (String(_url).endsWith('/playlists') && init?.method === 'POST') {
        return new Response(JSON.stringify({ summary: mockSummary, items: [] }), {
          status: 201,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (String(_url).endsWith('/playlists/pl-1') && (!init?.method || init.method === 'GET')) {
        return new Response(JSON.stringify({ summary: mockSummary, items: [] }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (String(_url).endsWith('/items') && init?.method === 'POST') {
        return new Response(
          JSON.stringify({ added_count: 2, skipped_duplicate_count: 0, summary: mockSummary }),
          { status: 200, headers: { 'Content-Type': 'application/json' } }
        );
      }
      return new Response(JSON.stringify(mockSummary), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    const created = await client.createPlaylist({
      name: 'Sci-Fi',
      is_public: false,
      items: { item_ids: [10, 14] },
    });
    expect(created.summary.id).toBe('pl-1');
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'Sci-Fi', is_public: false, items: { item_ids: [10, 14] } }),
      })
    );

    const detail = await client.getPlaylist('pl-1');
    expect(detail.summary.name).toBe('Sci-Fi');
    expect(fetchSpy).toHaveBeenCalledWith('/api/v1/playlists/pl-1', expect.any(Object));

    const updated = await client.updatePlaylist('pl-1', { name: 'Updated', is_public: true });
    expect(updated.id).toBe('pl-1');
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ name: 'Updated', is_public: true }),
      })
    );

    const added = await client.addPlaylistItems('pl-1', { show: { show_id: 50, season_number: 1 } });
    expect(added.added_count).toBe(2);
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1/items',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ show: { show_id: 50, season_number: 1 } }),
      })
    );

    await client.reorderPlaylistItems('pl-1', [14, 10]);
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1/items/order',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ item_ids: [14, 10] }),
      })
    );

    await client.removePlaylistItem('pl-1', 10);
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1/items/10',
      expect.objectContaining({ method: 'DELETE' })
    );

    await client.deletePlaylist('pl-1');
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1',
      expect.objectContaining({ method: 'DELETE' })
    );
  });
});

describe('Player, Storyboard & Watch-State API methods', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('appendMediaAuthParams preserves existing query params and appends token + unlocked', () => {
    const client = new ApiClient('http://localhost:8492');
    client.setToken('jwt-abc');
    client.storeUnlockToken('lib-priv', 'unlock-1');

    expect(client.appendMediaAuthParams('/api/v1/media/42/storyboard/0.jpg')).toBe(
      'http://localhost:8492/api/v1/media/42/storyboard/0.jpg?token=jwt-abc&unlocked=unlock-1'
    );
    expect(client.appendMediaAuthParams('/api/v1/media/42/frame?t=12&w=320')).toBe(
      'http://localhost:8492/api/v1/media/42/frame?t=12&w=320&token=jwt-abc&unlocked=unlock-1'
    );
  });

  it('builds HLS master URLs with codec, start_time, audio_track, and auth params', () => {
    const client = new ApiClient('');
    client.setToken('jwt-xyz');
    const url = client.getHlsMasterUrl(99, {
      videoCodec: 'none',
      audioCodec: 'aac',
      startTime: 125.8,
      audioTrack: 2,
    });
    expect(url).toBe(
      '/api/v1/media/99/hls/master.m3u8?video_codec=none&audio_codec=aac&start_time=125.8&audio_track=2&token=jwt-xyz'
    );
  });

  it('snaps getFramePreviewUrl timestamps down to 2-second increments', () => {
    const client = new ApiClient('');
    client.setToken('jwt-1');
    expect(client.getFramePreviewUrl(7, 15.9, 320)).toBe(
      '/api/v1/media/7/frame?t=14&w=320&token=jwt-1'
    );
  });

  it('chunks setPlaybackStatesBatch into <= 500 unique item IDs per PUT', async () => {
    const fetchMock = vi.fn().mockImplementation(async (_url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body));
      return new Response(
        JSON.stringify({ updated: body.item_ids.length, watched: body.watched }),
        { status: 200, headers: { 'Content-Type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const client = new ApiClient('');
    const ids = Array.from({ length: 620 }, (_, i) => i + 1);
    const res = await client.setPlaybackStatesBatch([...ids, 1, 2], true);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const firstBody = JSON.parse(String(fetchMock.mock.calls[0][1]?.body));
    const secondBody = JSON.parse(String(fetchMock.mock.calls[1][1]?.body));
    expect(firstBody.item_ids).toHaveLength(500);
    expect(secondBody.item_ids).toHaveLength(120);
    expect(res).toEqual({ updated: 620, watched: true });
  });

  it('parses getStoryboard 200 ready, 202 Retry-After generating, and 404 unavailable', async () => {
    const client = new ApiClient('');
    const manifest = {
      interval_ms: 10000,
      tile_width: 160,
      tile_height: 90,
      columns: 10,
      rows: 10,
      total_tiles: 12,
      sheets: [{ index: 0, url: '/api/v1/media/5/storyboard/0.jpg', first_tile: 0, tile_count: 12 }],
    };

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ status: 'generating' }), {
          status: 202,
          headers: { 'Content-Type': 'application/json', 'Retry-After': '7' },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify(manifest), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: 'Not found' }), {
          status: 404,
          headers: { 'Content-Type': 'application/json' },
        })
      );
    vi.stubGlobal('fetch', fetchMock);

    expect(await client.getStoryboard(5)).toEqual({
      status: 'generating',
      retryAfterMs: 7000,
    });
    expect(await client.getStoryboard(5)).toEqual({
      status: 'ready',
      manifest,
    });
    expect(await client.getStoryboard(5)).toEqual({ status: 'unavailable' });
  });

  it('handles setPlaybackState, getAudioTracks, deleteHlsSession, and getStoryboardSheetUrl', async () => {
    const client = new ApiClient('');
    client.setToken('jwt-sess');
    client.storeUnlockToken('lib-a', 'unlock-a');

    expect(client.getStoryboardSheetUrl('/api/v1/media/5/storyboard/0.jpg')).toBe(
      '/api/v1/media/5/storyboard/0.jpg?token=jwt-sess&unlocked=unlock-a'
    );

    const mockState = {
      user_id: 'u-1',
      media_item_id: 12,
      playback_position_seconds: 0,
      watch_state: 'completed' as const,
      last_watched_at: 1700000000,
      play_count: 1,
    };
    const mockTracks = [
      {
        audio_index: 0,
        stream_index: 1,
        codec: 'aac',
        channels: 2,
        language: 'eng',
        is_default: true,
        is_commentary: false,
      },
    ];

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify(mockState), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify(mockTracks), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      )
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal('fetch', fetchMock);

    const stateRes = await client.setPlaybackState(12, true);
    expect(stateRes).toEqual(mockState);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/playback/states/12');
    expect(fetchMock.mock.calls[0][1]).toMatchObject({
      method: 'PUT',
      body: JSON.stringify({ watched: true }),
    });

    const tracksRes = await client.getAudioTracks(12);
    expect(tracksRes).toEqual(mockTracks);
    expect(fetchMock.mock.calls[1][0]).toBe('/api/v1/items/12/audio-tracks');

    await client.deleteHlsSession(12, 'sess/1');
    expect(fetchMock.mock.calls[2][0]).toBe('/api/v1/media/12/hls/sessions/sess%2F1');
    expect(fetchMock.mock.calls[2][1]?.method).toBe('DELETE');
  });

  it('logs out and returns unavailable on getStoryboard 401, and short-circuits empty batch', async () => {
    const client = new ApiClient('');
    client.setToken('jwt-expired');
    const fetchMock = vi.fn().mockResolvedValueOnce(new Response(null, { status: 401 }));
    vi.stubGlobal('fetch', fetchMock);

    expect(await client.getStoryboard(5)).toEqual({ status: 'unavailable' });
    expect(client.getToken()).toBeNull();

    expect(await client.setPlaybackStatesBatch([0, -3], false)).toEqual({
      updated: 0,
      watched: false,
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});





