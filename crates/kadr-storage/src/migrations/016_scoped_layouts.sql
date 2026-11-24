CREATE TABLE IF NOT EXISTS layout_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    screen_id TEXT NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE CHECK (user_id IS NULL OR length(user_id) > 0),
    target_device TEXT NOT NULL CHECK (target_device IN ('all', 'tv', 'web', 'mobile')),
    layout_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_layout_overrides_scope
    ON layout_overrides(screen_id, IFNULL(user_id, ''), target_device);

CREATE INDEX IF NOT EXISTS idx_layout_overrides_user
    ON layout_overrides(user_id)
    WHERE user_id IS NOT NULL;
