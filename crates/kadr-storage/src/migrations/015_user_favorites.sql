CREATE TABLE IF NOT EXISTS user_favorites (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    favorited_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_user_favorites_user_recent
    ON user_favorites(user_id, favorited_at DESC, media_item_id DESC);
