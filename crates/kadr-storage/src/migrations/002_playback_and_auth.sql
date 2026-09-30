CREATE TABLE user_playback_states (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playback_position_seconds INTEGER NOT NULL DEFAULT 0,
    watch_state TEXT NOT NULL DEFAULT 'unwatched',
    last_watched_at INTEGER NOT NULL,
    play_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, media_item_id)
);

CREATE INDEX idx_playback_lookup ON user_playback_states(user_id, watch_state, last_watched_at DESC);
