CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    manifest TEXT NOT NULL,
    granted_permissions TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL
);

ALTER TABLE libraries ADD COLUMN plugin_id TEXT DEFAULT NULL;
CREATE INDEX IF NOT EXISTS idx_libraries_plugin ON libraries(plugin_id);
