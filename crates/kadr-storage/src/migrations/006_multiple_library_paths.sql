CREATE TABLE IF NOT EXISTS library_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(library_id, path)
);

-- Seed existing single library paths into library_paths table
INSERT OR IGNORE INTO library_paths (library_id, path, created_at)
SELECT id, path, created_at FROM libraries WHERE path IS NOT NULL AND path != '';

CREATE INDEX IF NOT EXISTS idx_library_paths_lib ON library_paths(library_id);
CREATE INDEX IF NOT EXISTS idx_library_paths_path ON library_paths(path);
