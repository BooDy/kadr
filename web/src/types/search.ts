export type SearchMediaType =
  | 'movie'
  | 'show'
  | 'season'
  | 'episode'
  | 'anime'
  | 'music'
  | 'home_videos'
  | 'audiobook'
  | 'live_channel'
  | 'unknown';

export interface SearchParams {
  q: string;
  item_type?: SearchMediaType | string;
  library_id?: string;
  limit?: number;
}

export interface SearchResultItem {
  id: number;
  library_id: string;
  library_name: string;
  item_type: SearchMediaType;
  title: string;
  original_title?: string | null;
  release_year?: number | null;
  overview?: string | null;
  poster_url: string;
  backdrop_url?: string | null;
  badge?: string | null;
  rating?: number | null;
  duration_seconds: number;
  playback_progress?: number | null;
  series_title?: string | null;
  season?: number | null;
  episode?: number | null;
  genres: string[];
  director?: string | null;
  is_favorite?: boolean;
}

export interface SearchResponse {
  query: string;
  total: number;
  items: SearchResultItem[];
}
