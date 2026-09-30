// crates/kadr-storage/tests/migration_002_test.rs
use kadr_core::models::{AuthClaims, PlaybackSession, PlaybackState, User, UserRole, WatchState};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};

#[tokio::test]
async fn test_migration_002_creates_playback_table() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let conn = pool.get().await.unwrap();
    conn.interact(|c| {
        // Verify user_playback_states table exists
        let table_exists: i32 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_playback_states'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_exists, 1);

        // Verify index exists
        let index_exists: i32 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_playback_lookup'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index_exists, 1);
    })
    .await
    .unwrap();
}

#[test]
fn test_auth_and_playback_models() {
    let role = UserRole::Admin;
    let user = User {
        id: "usr_1".to_string(),
        username: "admin".to_string(),
        pin_hash: "hash".to_string(),
        role,
        created_at: 1700000000,
    };
    let watch_state = WatchState::InProgress;
    let playback_state = PlaybackState {
        user_id: user.id.clone(),
        media_item_id: 1,
        playback_position_seconds: 120,
        watch_state,
        last_watched_at: 1700000100,
        play_count: 1,
    };
    let session = PlaybackSession {
        session_id: "sess_1".to_string(),
        user_id: user.id.clone(),
        media_item_id: 1,
        duration_seconds: 600,
        current_position_seconds: 120,
        started_at: 1700000000,
        last_heartbeat_at: 1700000100,
    };
    let claims = AuthClaims {
        sub: user.id.clone(),
        username: user.username.clone(),
        role: user.role,
        exp: 2000000000,
        iat: 1700000000,
    };

    assert_eq!(user.role, UserRole::Admin);
    assert_eq!(playback_state.watch_state, WatchState::InProgress);
    assert_eq!(session.session_id, "sess_1");
    assert_eq!(claims.sub, "usr_1");
}
