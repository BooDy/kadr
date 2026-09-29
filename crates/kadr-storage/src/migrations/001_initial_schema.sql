CREATE TABLE libraries (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT UNIQUE NOT NULL,
    media_type TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    pin_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'standard',
    created_at INTEGER NOT NULL
);

CREATE TABLE media_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    item_type TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    release_year INTEGER,
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    file_path TEXT UNIQUE NOT NULL,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    resolution TEXT,
    video_codec TEXT,
    audio_codec TEXT,
    audio_channels INTEGER,
    container TEXT,
    metadata JSON NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_media_library_type ON media_items(library_id, item_type);
CREATE INDEX idx_media_added ON media_items(added_at DESC);
CREATE INDEX idx_media_path ON media_items(file_path);
CREATE INDEX idx_media_title_year ON media_items(title, release_year);
