import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { ApiClient, ApiError } from './client';
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
});


