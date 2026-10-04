import type {
  CardViewModel,
  CreateLibraryPayload,
  CreateUserPayload,
  DownloadSubtitleRequest,
  FsBrowseResponse,
  ItemDetailsPayload,
  Library,
  OnlineSubtitleSearchResponse,
  PlaybackSessionResponse,
  PlaybackState,
  ScanResultResponse,
  ScreenLayout,
  ScreenSummary,
  SubtitleTrack,
  SystemConfig,
  TelemetrySnapshot,
  UnlockLibraryResponse,
  UpdateConfigPayload,
  User,
  WidgetDataResponse,
} from '../types';

export class ApiError extends Error {
  public status: number;
  public details?: unknown;

  constructor(status: number, message: string, details?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.details = details;
  }
}

export const TOKEN_STORAGE_KEY = 'kadr_token';
export const USER_STORAGE_KEY = 'kadr_user';
export const UNLOCKED_LIBRARIES_STORAGE_KEY = 'kadr_unlocked_libraries';

export class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = '') {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  // Token & User Management
  public getToken(): string | null {
    if (typeof localStorage === 'undefined') return null;
    return localStorage.getItem(TOKEN_STORAGE_KEY);
  }

  public setToken(token: string | null): void {
    if (typeof localStorage === 'undefined') return;
    if (token) {
      localStorage.setItem(TOKEN_STORAGE_KEY, token);
    } else {
      localStorage.removeItem(TOKEN_STORAGE_KEY);
    }
  }

  public getUser(): User | null {
    if (typeof localStorage === 'undefined') return null;
    const raw = localStorage.getItem(USER_STORAGE_KEY);
    if (!raw) return null;
    try {
      return JSON.parse(raw) as User;
    } catch {
      return null;
    }
  }

  public setUser(user: User | null): void {
    if (typeof localStorage === 'undefined') return;
    if (user) {
      localStorage.setItem(USER_STORAGE_KEY, JSON.stringify(user));
    } else {
      localStorage.removeItem(USER_STORAGE_KEY);
    }
  }

  // Unlocked Libraries Management (sessionStorage)
  public getUnlockedTokens(): Record<string, string> {
    if (typeof sessionStorage === 'undefined') return {};
    const raw = sessionStorage.getItem(UNLOCKED_LIBRARIES_STORAGE_KEY);
    if (!raw) return {};
    try {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === 'object') {
        if (Array.isArray(parsed)) {
          const result: Record<string, string> = {};
          for (const item of parsed) {
            if (typeof item === 'string') {
              result[item] = item;
            } else if (item && typeof item === 'object' && item.library_id && item.token) {
              result[item.library_id] = item.token;
            }
          }
          return result;
        }
        const result: Record<string, string> = {};
        for (const [key, val] of Object.entries(parsed)) {
          if (typeof val === 'string') {
            result[key] = val;
          } else if (val && typeof val === 'object' && 'token' in val && typeof (val as { token: unknown }).token === 'string') {
            result[key] = (val as { token: string }).token;
          }
        }
        return result;
      }
    } catch {
      return {};
    }
    return {};
  }

  public getUnlockedLibraryIds(): string[] {
    return Object.keys(this.getUnlockedTokens());
  }

  public isLibraryUnlocked(libraryId: string): boolean {
    return Boolean(this.getUnlockedTokens()[libraryId]);
  }

  public getUnlockToken(libraryId: string): string | null {
    return this.getUnlockedTokens()[libraryId] || null;
  }

  public storeUnlockToken(libraryId: string, token: string): void {
    if (typeof sessionStorage === 'undefined') return;
    const current = this.getUnlockedTokens();
    current[libraryId] = token;
    sessionStorage.setItem(UNLOCKED_LIBRARIES_STORAGE_KEY, JSON.stringify(current));
  }

  public clearUnlockedToken(libraryId: string): void {
    if (typeof sessionStorage === 'undefined') return;
    const current = this.getUnlockedTokens();
    delete current[libraryId];
    sessionStorage.setItem(UNLOCKED_LIBRARIES_STORAGE_KEY, JSON.stringify(current));
  }

  public clearUnlockedTokens(): void {
    if (typeof sessionStorage === 'undefined') return;
    sessionStorage.removeItem(UNLOCKED_LIBRARIES_STORAGE_KEY);
  }

  public logout(): void {
    this.setToken(null);
    this.setUser(null);
    this.clearUnlockedTokens();
  }

  // HTTP Request Helper
  public async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const headers: Record<string, string> = {
      Accept: 'application/json',
    };

    if (
      options.body &&
      typeof options.body === 'string'
    ) {
      headers['Content-Type'] = 'application/json';
    }

    if (options.headers) {
      if (typeof Headers !== 'undefined' && options.headers instanceof Headers) {
        options.headers.forEach((val, key) => {
          headers[key] = val;
        });
      } else if (Array.isArray(options.headers)) {
        options.headers.forEach(([key, val]) => {
          headers[key] = val;
        });
      } else {
        Object.assign(headers, options.headers);
      }
    }

    const token = this.getToken();
    if (token && !headers['Authorization'] && !headers['authorization']) {
      headers['Authorization'] = `Bearer ${token}`;
    }

    const unlockedTokens = Object.values(this.getUnlockedTokens()).filter(Boolean);
    if (unlockedTokens.length > 0 && !headers['X-Kadr-Unlocked'] && !headers['x-kadr-unlocked']) {
      headers['X-Kadr-Unlocked'] = unlockedTokens.join(',');
    }

    const response = await fetch(url, {
      ...options,
      headers,
    });

    if (!response.ok) {
      if (response.status === 401) {
        this.logout();
      }

      let errorMessage = `HTTP error ${response.status}: ${response.statusText}`;
      let errorDetails: unknown = undefined;

      try {
        const contentType = response.headers.get('content-type') || '';
        if (contentType.includes('application/json')) {
          const json = await response.json();
          errorDetails = json;
          if (json && typeof json === 'object') {
            if ('error' in json && typeof json.error === 'string') {
              errorMessage = json.error;
            } else if ('message' in json && typeof json.message === 'string') {
              errorMessage = json.message;
            }
          }
        } else {
          const text = await response.text();
          if (text) {
            errorMessage = text;
          }
        }
      } catch {
        // Fall back to default error message
      }

      throw new ApiError(response.status, errorMessage, errorDetails);
    }

    if (response.status === 204) {
      return undefined as unknown as T;
    }

    if (typeof response.text === 'function') {
      const text = await response.text();
      return (text ? JSON.parse(text) : undefined) as T;
    }

    if (typeof response.json === 'function') {
      return (await response.json()) as T;
    }

    return undefined as unknown as T;
  }

  // URL Resolution Methods
  public getStreamUrl(itemId: number, unlocked?: string): string {
    const token = this.getToken();
    const base = `${this.baseUrl}/api/v1/stream/${itemId}`;
    const params = new URLSearchParams();
    if (token) {
      params.append('token', token);
    }
    const unlockedTokens = unlocked
      ? [unlocked]
      : Object.values(this.getUnlockedTokens()).filter(Boolean);
    if (unlockedTokens.length > 0) {
      params.append('unlocked', unlockedTokens.join(','));
    }
    const query = params.toString();
    return query ? `${base}?${query}` : base;
  }

  public getSubtitleStreamUrl(subtitleId: number): string {
    const token = this.getToken();
    const base = `${this.baseUrl}/api/v1/subtitles/${subtitleId}/stream.vtt`;
    return token ? `${base}?token=${encodeURIComponent(token)}` : base;
  }

  public getArtworkUrl(
    itemId: number,
    type: 'poster' | 'backdrop'
  ): string {
    return `${this.baseUrl}/api/v1/artwork/${itemId}/${type}`;
  }

  // Authentication
  public async getProfiles(): Promise<User[]> {
    return this.request<User[]>('/api/v1/users/profiles');
  }

  public async loginWithPin(
    pin: string,
    userId: string = 'admin'
  ): Promise<{ token: string; user: User }> {
    const res = await this.request<{
      token: string;
      user_id: string;
      username: string;
      role: 'admin' | 'standard';
    }>('/api/v1/auth/pin', {
      method: 'POST',
      body: JSON.stringify({ user_id: userId, pin }),
    });

    const user: User = {
      id: res.user_id,
      username: res.username,
      role: res.role,
    };

    this.setToken(res.token);
    this.setUser(user);

    return { token: res.token, user };
  }

  public async getCurrentUser(): Promise<User> {
    return this.request<User>('/api/v1/auth/me');
  }

  // Screens & Widgets
  public async getScreens(): Promise<ScreenSummary[]> {
    return this.request<ScreenSummary[]>('/api/v1/screens');
  }

  public async getScreen(screenId: string): Promise<ScreenLayout> {
    return this.request<ScreenLayout>(`/api/v1/screens/${encodeURIComponent(screenId)}`);
  }

  public async getWidgetData(
    widgetId: string,
    page: number = 0,
    limit: number = 20
  ): Promise<CardViewModel[]> {
    const offset = page * limit;
    const res = await this.request<WidgetDataResponse>(
      `/api/v1/widgets/${encodeURIComponent(widgetId)}/data?offset=${offset}&limit=${limit}`
    );
    return res.items || [];
  }

  public async getItemDetails(itemId: number): Promise<ItemDetailsPayload> {
    return this.request<ItemDetailsPayload>(`/api/v1/items/${itemId}/details`);
  }

  // Subtitles
  public async getSubtitles(itemId: number): Promise<SubtitleTrack[]> {
    return this.request<SubtitleTrack[]>(`/api/v1/items/${itemId}/subtitles`);
  }

  public async searchSubtitles(
    itemId: number,
    languages?: string
  ): Promise<OnlineSubtitleSearchResponse> {
    const query = languages ? `?languages=${encodeURIComponent(languages)}` : '';
    return this.request<OnlineSubtitleSearchResponse>(
      `/api/v1/subtitles/${itemId}/search${query}`
    );
  }

  public async downloadSubtitle(
    itemId: number,
    req: DownloadSubtitleRequest
  ): Promise<SubtitleTrack> {
    return this.request<SubtitleTrack>(`/api/v1/subtitles/${itemId}/download`, {
      method: 'POST',
      body: JSON.stringify(req),
    });
  }

  public async deleteSubtitle(
    subtitleId: number
  ): Promise<{ deleted: boolean }> {
    return this.request<{ deleted: boolean }>(`/api/v1/subtitles/${subtitleId}`, {
      method: 'DELETE',
    });
  }

  // Playback Sessions & Heartbeat
  public async createPlaybackSession(
    itemId: number
  ): Promise<PlaybackSessionResponse> {
    return this.request<PlaybackSessionResponse>('/api/v1/playback/sessions', {
      method: 'POST',
      body: JSON.stringify({ media_item_id: itemId }),
    });
  }

  public async sendPlaybackHeartbeat(
    sessionId: string,
    positionSeconds: number
  ): Promise<void> {
    await this.request<void>(
      `/api/v1/playback/${encodeURIComponent(sessionId)}/progress`,
      {
        method: 'POST',
        body: JSON.stringify({ position_seconds: positionSeconds }),
      }
    );
  }

  public async closePlaybackSession(sessionId: string): Promise<void> {
    await this.request<void>(
      `/api/v1/playback/sessions/${encodeURIComponent(sessionId)}`,
      {
        method: 'DELETE',
      }
    );
  }

  public async getPlaybackState(itemId: number): Promise<PlaybackState> {
    return this.request<PlaybackState>(`/api/v1/playback/states/${itemId}`);
  }

  // System Telemetry
  public async getTelemetry(): Promise<TelemetrySnapshot> {
    return this.request<TelemetrySnapshot>('/api/v1/system/telemetry');
  }

  // Libraries Management
  public async getLibraries(): Promise<Library[]> {
    return this.request<Library[]>('/api/v1/libraries');
  }

  public async unlockLibrary(
    id: string,
    pin: string
  ): Promise<UnlockLibraryResponse> {
    const res = await this.request<UnlockLibraryResponse>(
      `/api/v1/libraries/${encodeURIComponent(id)}/unlock`,
      {
        method: 'POST',
        body: JSON.stringify({ pin }),
      }
    );
    this.storeUnlockToken(res.library_id, res.token);
    return res;
  }

  public async createLibrary(payload: CreateLibraryPayload): Promise<Library> {
    return this.request<Library>('/api/v1/libraries', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  public async deleteLibrary(id: string): Promise<void> {
    await this.request<void>(`/api/v1/libraries/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    });
  }

  public async scanLibrary(id: string): Promise<ScanResultResponse> {
    return this.request<ScanResultResponse>(
      `/api/v1/libraries/${encodeURIComponent(id)}/scan`,
      {
        method: 'POST',
      }
    );
  }

  public async addLibraryPath(libraryId: string, path: string): Promise<void> {
    await this.request<void>(`/api/v1/libraries/${encodeURIComponent(libraryId)}/paths`, {
      method: 'POST',
      body: JSON.stringify({ path }),
    });
  }

  public async removeLibraryPath(libraryId: string, path: string): Promise<void> {
    await this.request<void>(
      `/api/v1/libraries/${encodeURIComponent(libraryId)}/paths?path=${encodeURIComponent(path)}`,
      {
        method: 'DELETE',
      }
    );
  }

  // Filesystem Browsing
  public async browseFilesystem(path?: string): Promise<FsBrowseResponse> {
    const query = path ? `?path=${encodeURIComponent(path)}` : '';
    return this.request<FsBrowseResponse>(`/api/v1/system/fs${query}`);
  }

  // System Configuration
  public async getSystemConfig(): Promise<SystemConfig> {
    return this.request<SystemConfig>('/api/v1/system/config');
  }

  public async updateSystemConfig(
    payload: UpdateConfigPayload
  ): Promise<SystemConfig> {
    return this.request<SystemConfig>('/api/v1/system/config', {
      method: 'PUT',
      body: JSON.stringify(payload),
    });
  }

  // User Management
  public async createUser(payload: CreateUserPayload): Promise<User> {
    return this.request<User>('/api/v1/users', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }
}

export const api = new ApiClient();
