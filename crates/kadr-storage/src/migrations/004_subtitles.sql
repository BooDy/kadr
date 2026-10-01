CREATE TABLE media_subtitles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    source TEXT NOT NULL,          -- 'sidecar', 'embedded', 'downloaded'
    language TEXT NOT NULL,        -- 'eng', 'ara', 'fre', 'und'
    title TEXT,                    -- 'English [SDH]', etc.
    format TEXT NOT NULL,          -- 'srt', 'vtt', 'ass', 'sub'
    file_path TEXT,                -- Disk path for sidecars or downloaded files
    stream_index INTEGER,          -- Container stream index for embedded tracks
    is_default INTEGER NOT NULL DEFAULT 0,
    is_forced INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_subtitles_item ON media_subtitles(media_item_id);
CREATE INDEX idx_subtitles_lang ON media_subtitles(media_item_id, language);
