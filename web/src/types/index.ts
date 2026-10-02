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

// Layout & AST Types
export interface ScreenSummary {
  id: string;
  title: string;
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
}

export type QueryMacro =
  | 'continue_watching'
  | 'recently_added'
  | 'top_rated'
  | { library_items: { library_id: string } }
  | { genre_shelf: { genre: string } }
  | { spotlight_item: { item_id?: number } }
  | { item_details: { item_id: number } };

export interface WidgetQueryBinding {
  macro_type: QueryMacro | string | Record<string, unknown>;
  limit: number;
  sort?: string;
}

export type WidgetNode =
  | {
      type: 'hero_banner';
      id: string;
      binding: WidgetQueryBinding;
      data?: CardViewModel;
    }
  | {
      type: 'carousel';
      id: string;
      title: string;
      binding: WidgetQueryBinding;
      items?: CardViewModel[];
      next_cursor?: string;
    }
  | {
      type: 'grid';
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

export interface OnlineSubtitleMatch {
  id: string;
  language: string;
  release_name?: string;
  hearing_impaired: boolean;
  format: string;
  download_count: number;
  rating?: number;
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
