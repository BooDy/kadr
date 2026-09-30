use deadpool_sqlite::Pool;
use rusqlite::params;
use kadr_core::models::{PlaybackState, WatchState};
use crate::error::Result;

#[derive(Clone)]
pub struct PlaybackRepository {
    pool: Pool,
}

fn map_playback_row(row: &rusqlite::Row) -> rusqlite::Result<PlaybackState> {
    let user_id: String = row.get(0)?;
    let media_item_id: i64 = row.get(1)?;
    let playback_position_seconds: i64 = row.get(2)?;
    let watch_state_str: String = row.get(3)?;
    let last_watched_at: i64 = row.get(4)?;
    let play_count: u32 = row.get(5)?;

    let watch_state = match watch_state_str.as_str() {
        "in_progress" => WatchState::InProgress,
        "completed" => WatchState::Completed,
        _ => WatchState::Unwatched,
    };

    Ok(PlaybackState {
        user_id,
        media_item_id,
        playback_position_seconds,
        watch_state,
        last_watched_at,
        play_count,
    })
}

impl PlaybackRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn upsert_progress(
        &self,
        user_id: &str,
        media_item_id: i64,
        position_seconds: i64,
        watch_state: WatchState,
        now: i64,
    ) -> Result<()> {
        let user_id = user_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let state_str = match watch_state {
                WatchState::Unwatched => "unwatched",
                WatchState::InProgress => "in_progress",
                WatchState::Completed => "completed",
            };

            c.execute(
                "INSERT INTO user_playback_states (
                    user_id, media_item_id, playback_position_seconds, watch_state, last_watched_at, play_count
                 ) VALUES (?1, ?2, ?3, ?4, ?5, CASE WHEN ?4 = 'completed' THEN 1 ELSE 0 END)
                 ON CONFLICT(user_id, media_item_id) DO UPDATE SET
                    playback_position_seconds = excluded.playback_position_seconds,
                    watch_state = excluded.watch_state,
                    last_watched_at = excluded.last_watched_at,
                    play_count = CASE
                        WHEN excluded.watch_state = 'completed' AND user_playback_states.watch_state != 'completed'
                        THEN user_playback_states.play_count + 1
                        ELSE user_playback_states.play_count
                    END",
                params![user_id, media_item_id, position_seconds, state_str, now],
            )?;
            Ok(())
        }).await?
    }

    pub async fn get_state(&self, user_id: &str, media_item_id: i64) -> Result<Option<PlaybackState>> {
        let user_id = user_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT user_id, media_item_id, playback_position_seconds, watch_state, last_watched_at, play_count
                 FROM user_playback_states
                 WHERE user_id = ?1 AND media_item_id = ?2",
            )?;
            let mut rows = stmt.query(params![user_id, media_item_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_playback_row(row)?))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn list_user_states(
        &self,
        user_id: &str,
        watch_state: Option<WatchState>,
        limit: usize,
    ) -> Result<Vec<PlaybackState>> {
        let user_id = user_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut list = Vec::new();
            if let Some(state) = watch_state {
                let state_str = match state {
                    WatchState::InProgress => "in_progress",
                    WatchState::Completed => "completed",
                    WatchState::Unwatched => "unwatched",
                };
                let mut stmt = c.prepare(
                    "SELECT user_id, media_item_id, playback_position_seconds, watch_state, last_watched_at, play_count
                     FROM user_playback_states
                     WHERE user_id = ?1 AND watch_state = ?2
                     ORDER BY last_watched_at DESC
                     LIMIT ?3",
                )?;
                let rows = stmt.query_map(params![user_id, state_str, limit as i64], map_playback_row)?;
                for r in rows {
                    list.push(r?);
                }
            } else {
                let mut stmt = c.prepare(
                    "SELECT user_id, media_item_id, playback_position_seconds, watch_state, last_watched_at, play_count
                     FROM user_playback_states
                     WHERE user_id = ?1
                     ORDER BY last_watched_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![user_id, limit as i64], map_playback_row)?;
                for r in rows {
                    list.push(r?);
                }
            }

            Ok(list)
        }).await?
    }
}
