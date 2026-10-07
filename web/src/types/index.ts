// User & Authentication Types
export type UserRole = 'admin' | 'standard';

export interface User {
  id: string;
  username: string;
  role: UserRole;
  created_at?: number;
}

export interface AuthResponse {
  token: string;
  user_id: string;
  username: string;
  role: UserRole;
}

// Media Types
export type MediaType =
  | 'movie'
  | 'show'
  | 'anime'
  | 'music'
  | 'home_videos'
  | 'audiobook'
  | 'unknown';

// Layout & AST Types
export interface ScreenSummary {
  id: string;
  title: string;
}

export interface CreateScreenPayload {
  id: string;
  title: string;
  description?: string;
}


export interface CardViewModel {
  id: number;
  title: string;
  subtitle?: string;
  poster_url?: string;
  backdrop_url?: string;
  media_type: string;
  playback_progress?: number;
  rating?: number;
  release_year?: number;
  badge?: string;
  season?: number;
  episode?: number;
  duration_seconds?: number;
}

export type QueryMacro =
  | 'continue_watching'
  | 'recently_added'
  | 'top_rated'
  | { library_items: { library_id: string } }
  | { genre_shelf: { genre: string } }
  | { spotlight_item: { item_id?: number } }
  | { item_details: { item_id: number } };

export interface WidgetFilterConfig {
  exclude_private?: boolean;
  exclude_library_ids?: string[];
  exclude_genres?: string[];
  max_age_days?: number;
  all_libraries?: boolean;
  library_id?: string;
}

export interface WidgetQueryBinding {
  macro_type: QueryMacro | string | Record<string, unknown>;
  limit: number;
  sort?: string;
  filters?: WidgetFilterConfig;
}

export type WidgetNode =
  | {
      type: 'hero_banner';
      display_type?: string;
      id: string;
      binding: WidgetQueryBinding;
      data?: CardViewModel;
    }
  | {
      type: 'carousel';
      display_type?: string;
      id: string;
      title: string;
      binding: WidgetQueryBinding;
      items?: CardViewModel[];
      next_cursor?: string;
    }
  | {
      type: 'grid';
      display_type?: string;
      id: string;
      title: string;
      binding: WidgetQueryBinding;
      columns: number;
      items?: CardViewModel[];
      next_cursor?: string;
      total_count?: number;
    }
  | {
      type: 'item_details';
      display_type?: string;
      id: string;
      item_id: number;
      details?: ItemDetailsPayload;
    };

export interface TechnicalInfo {
  duration_seconds: number;
  resolution?: string;
  video_codec?: string;
  audio_codec?: string;
  audio_channels?: number;
  container?: string;
}

export interface ItemDetailsPayload {
  card: CardViewModel;
  overview?: string;
  genres: string[];
  duration_seconds?: number;
  technical?: TechnicalInfo;
  stream_url: string;
  resume_position_seconds?: number;
  episodes?: CardViewModel[];
  library_id?: string;
  folder_path?: string;
}

export interface ScreenLayout {
  id: string;
  title: string;
  widgets: WidgetNode[];
}

export interface WidgetDataResponse {
  widget_id: string;
  items: CardViewModel[];
  next_cursor?: string;
  total_count?: number;
}

// Subtitles
export type SubtitleSource = 'sidecar' | 'embedded' | 'downloaded';
export type SubtitleFormat = 'srt' | 'vtt' | 'ass' | 'sub' | 'unknown';

export interface SubtitleTrack {
  id: number;
  media_item_id: number;
  source: SubtitleSource;
  language: string;
  title?: string;
  format: SubtitleFormat;
  is_default: boolean;
  is_forced: boolean;
  stream_url: string;
}

export interface SubtitleSearchResult {
  id: string;
  language: string;
  format: string;
  release_name?: string;
  hearing_impaired?: boolean;
  download_count?: number;
  rating?: number;
}

export interface OnlineSubtitleMatch extends SubtitleSearchResult {
  hearing_impaired: boolean;
  download_count: number;
}

export interface OnlineSubtitleSearchResponse {
  configured: boolean;
  matches: OnlineSubtitleMatch[];
}

export interface DownloadSubtitleRequest {
  file_id: string;
  language: string;
  title?: string;
  is_forced?: boolean;
}

// Playback
export interface PlaybackSessionResponse {
  session_id: string;
  media_item_id: number;
  duration_seconds: number;
  resume_position_seconds: number;
}

export interface PlaybackState {
  user_id: string;
  media_item_id: number;
  playback_position_seconds: number;
  watch_state: 'unwatched' | 'in_progress' | 'completed';
  last_watched_at: number;
  play_count: number;
}

// Telemetry & Events
export interface TelemetrySnapshot {
  active_sessions_count: number;
  rss_memory_bytes: number;
  db_size_bytes: number;
  wal_size_bytes: number;
  timestamp: number;
}

export type SystemEvent =
  | {
      type: 'library:updated';
      payload: {
        library_id: string;
        item_count: number;
        timestamp: number;
      };
    }
  | {
      type: 'layout:changed';
      payload: {
        screen_id: string;
        timestamp: number;
      };
    }
  | {
      type: 'subtitle:downloaded';
      payload: {
        item_id: number;
        subtitle_id: number;
        language: string;
        timestamp: number;
      };
    }
  | {
      type: 'session:synced';
      payload: {
        session_id: string;
        item_id: number;
        user_id: string;
        position_seconds: number;
        timestamp: number;
      };
    }
  | {
      type: 'system:telemetry';
      payload: TelemetrySnapshot;
    };

// Filesystem Browsing Types
export interface FsDirectoryEntry {
  name: string;
  path: string;
}

export interface FsShortcut {
  name: string;
  path: string;
}

export interface FsBrowseResponse {
  current_path: string;
  parent_path: string | null;
  directories: FsDirectoryEntry[];
  shortcuts: FsShortcut[];
}

// Library Management Types
export interface FolderEntry {
  name: string;
  path: string;
  item_count: number;
}

export interface BreadcrumbItem {
  name: string;
  path: string;
}

export interface FolderImageEntry {
  name: string;
  path: string;
  url: string;
  size_bytes: number;
}

export interface LibraryFolderResponse {
  library_id: string;
  library_name: string;
  current_path: string;
  parent_path: string | null;
  breadcrumbs: BreadcrumbItem[];
  directories: FolderEntry[];
  items: CardViewModel[];
  images?: FolderImageEntry[];
}

export interface Library {
  id: string;
  name: string;
  path: string;
  paths?: string[];
  media_type: MediaType | 'Movie' | 'Episode' | 'Show' | 'Music' | 'Other' | string;
  is_private: boolean;
  created_at: number;
}

export interface CreateLibraryPayload {
  name: string;
  path?: string;
  paths?: string[];
  media_type: MediaType | 'Movie' | 'Episode' | string;
  is_private?: boolean;
  pin?: string;
}

export interface UpdateLibraryPayload {
  name: string;
}


export interface UnlockLibraryResponse {
  library_id: string;
  token: string;
  expires_at: number;
}

export interface ScanResultResponse {
  library_id: string;
  files_scanned: number;
  queued: boolean;
}

export interface DiscoveryResponse {
  app: 'kadr';
  server_id: string;
  name: string;
  version: string;
  protocol_version: number;
  port: number;
  setup_completed: boolean;
  status: 'online';
}

// System Configuration Types
export interface SystemConfig {
  name: string;
  host: string;
  port: number;
  data_dir: string;
  web_dir?: string;
  database_path: string;
  max_readers: number;
  debounce_millis: number;
  use_ffprobe: boolean;
}

export interface UpdateConfigPayload {
  name?: string;
  host?: string;
  port?: number;
  debounce_millis?: number;
  use_ffprobe?: boolean;
}

export interface CreateUserPayload {
  username: string;
  pin: string;
  role: UserRole;
}
