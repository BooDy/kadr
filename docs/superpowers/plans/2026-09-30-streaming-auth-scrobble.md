# Kadr Milestone 2: HTTP 206 Streaming Engine, PIN Auth & Playback Synchronization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement multi-user tenancy with 4-digit quick PIN authentication (Argon2id + rate limiting + JWT), native zero-copy HTTP 206 partial content streaming, and active playback session tracking with automated scrobble synchronization.

**Architecture:** Extend `kadr-core` with auth and playback models; add SQLite migration `002_playback_and_auth.sql` and typed `UserRepository` / `PlaybackRepository` in `kadr-storage`; equip `kadr-server` with modular Axum subsystems for PIN verification (`argon2`), token-bucket IP rate limiting, JWT token management (`jsonwebtoken`), RFC 7233 byte-range streaming, in-memory `SessionRegistry`, and progress scrobble threshold evaluation.

**Tech Stack:** Rust (edition 2021), `axum`, `tokio`, `tokio-util`, `argon2`, `jsonwebtoken`, `rusqlite`, `deadpool-sqlite`, `serde` + `serde_json`, `tower` + `tower-http`, `thiserror`, `tracing`.

## Global Constraints

- RSS memory usage must remain $\le 30\text{ MB}$ under idle and active streaming workloads.
- Streaming Time-to-First-Byte (TTFB) must be $<100\text{ ms}$ on local range requests via zero-copy async file streaming (`tokio_util::io::ReaderStream` with 64 KiB buffer).
- Zero-external runtime dependencies baseline (compilable against musl).
- SQLite operations must maintain WAL mode, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.
- Rate limiting enforces a maximum of 5 failed PIN attempts per IP within 300 seconds before a 5-minute lockout (`HTTP 429 Too Many Requests` with `Retry-After: 300`).
- Scrobble state updates: $>60\text{s}$ or $>2\% \implies \text{in\_progress}$, $\ge 90\% \implies \text{completed}$.

---

### Task 1: Domain Models & Migration 002 (`user_playback_states`)

**Files:**
- Modify: `crates/kadr-core/src/models.rs`
- Modify: `crates/kadr-core/src/lib.rs`
- Create: `crates/kadr-storage/src/migrations/002_playback_and_auth.sql`
- Modify: `crates/kadr-storage/src/migrations.rs`
- Test: `crates/kadr-storage/tests/migration_002_test.rs`

**Interfaces:**
- Consumes: `kadr-core`, `kadr-storage`
- Produces: `UserRole`, `User`, `WatchState`, `PlaybackState`, `PlaybackSession`, `AuthClaims`, updated `run_migrations`

- [ ] **Step 1: Write failing migration and domain model test**

```rust
// crates/kadr-storage/tests/migration_002_test.rs
use kadr_core::models::{UserRole, WatchState, PlaybackState, PlaybackSession, AuthClaims};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};

#[tokio::test]
async fn test_migration_002_creates_playback_table() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let conn = pool.get().await.unwrap();
    conn.interact(|c| {
        // Verify user_playback_states table exists
        let table_exists: i32 = c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_playback_states'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(table_exists, 1);

        // Verify index exists
        let index_exists: i32 = c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_playback_lookup'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(index_exists, 1);
    }).await.unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-storage --test migration_002_test`
Expected: FAIL (types or migration not found)

- [ ] **Step 3: Implement domain models and migration 002**

Update `crates/kadr-core/src/models.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    #[serde(alias = "Admin", alias = "ADMIN")]
    Admin,
    #[serde(alias = "Standard", alias = "STANDARD")]
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub pin_hash: String,
    pub role: UserRole,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    Unwatched,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackState {
    pub user_id: String,
    pub media_item_id: i64,
    pub playback_position_seconds: i64,
    pub watch_state: WatchState,
    pub last_watched_at: i64,
    pub play_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub session_id: String,
    pub user_id: String,
    pub media_item_id: i64,
    pub duration_seconds: i64,
    pub current_position_seconds: i64,
    pub started_at: i64,
    pub last_heartbeat_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub username: String,
    pub role: UserRole,
    pub exp: usize,
    pub iat: usize,
}
```

Create `crates/kadr-storage/src/migrations/002_playback_and_auth.sql`:
```sql
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
```

Update `crates/kadr-storage/src/migrations.rs`:
```rust
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/001_initial_schema.sql")),
        M::up(include_str!("migrations/002_playback_and_auth.sql")),
    ])
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-storage --test migration_002_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-core crates/kadr-storage
git commit -m "feat(core,storage): add auth/playback models and migration 002"
```

---

### Task 2: Repositories for Users & Playback States (`kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/repos/user_repo.rs`
- Create: `crates/kadr-storage/src/repos/playback_repo.rs`
- Modify: `crates/kadr-storage/src/repos/mod.rs`
- Modify: `crates/kadr-storage/src/lib.rs`
- Test: `crates/kadr-storage/tests/user_and_playback_repos_test.rs`

**Interfaces:**
- Consumes: `kadr-core`, `kadr-storage`
- Produces: `UserRepository`, `PlaybackRepository`

- [ ] **Step 1: Write failing repository test**

```rust
// crates/kadr-storage/tests/user_and_playback_repos_test.rs
use kadr_core::models::{User, UserRole, WatchState, Library, MediaType, MediaItem, TechnicalInfo, MediaMetadata};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, LibraryRepository, MediaItemRepository};
use std::path::PathBuf;

#[tokio::test]
async fn test_user_and_playback_repositories() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // 1. Create User
    let user = User {
        id: "u123".to_string(),
        username: "ahmed".to_string(),
        pin_hash: "argon2id_hash_sample".to_string(),
        role: UserRole::Standard,
        created_at: 1700000000,
    };
    user_repo.create(&user).await.unwrap();

    let fetched = user_repo.get_by_id("u123").await.unwrap().expect("user not found");
    assert_eq!(fetched.username, "ahmed");
    assert_eq!(fetched.role, UserRole::Standard);

    // 2. Setup library and media item for foreign key reference
    lib_repo.create(&Library {
        id: "lib1".to_string(),
        name: "Films".to_string(),
        path: PathBuf::from("/media"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    }).await.unwrap();

    media_repo.upsert_batch(&[MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "Test Movie".to_string(),
        original_title: None,
        release_year: Some(2020),
        added_at: 1700000000,
        file_path: PathBuf::from("/media/test.mp4"),
        file_name: "test.mp4".to_string(),
        file_size: 1000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata::default(),
    }]).await.unwrap();

    let items = media_repo.list_by_library("lib1", 1, 0).await.unwrap();
    let item_id = items[0].id.unwrap();

    // 3. Upsert playback progress
    playback_repo.upsert_progress("u123", item_id, 350, WatchState::InProgress, 1700000500).await.unwrap();

    let state = playback_repo.get_state("u123", item_id).await.unwrap().expect("state not found");
    assert_eq!(state.playback_position_seconds, 350);
    assert_eq!(state.watch_state, WatchState::InProgress);

    // 4. Update to completed and increment play count
    playback_repo.upsert_progress("u123", item_id, 5000, WatchState::Completed, 1700001000).await.unwrap();
    let state_completed = playback_repo.get_state("u123", item_id).await.unwrap().unwrap();
    assert_eq!(state_completed.watch_state, WatchState::Completed);
    assert_eq!(state_completed.play_count, 1);

    // 5. Query user in-progress states
    let in_progress = playback_repo.list_user_states("u123", Some(WatchState::InProgress), 10).await.unwrap();
    assert_eq!(in_progress.len(), 0);
    let completed = playback_repo.list_user_states("u123", Some(WatchState::Completed), 10).await.unwrap();
    assert_eq!(completed.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-storage --test user_and_playback_repos_test`
Expected: FAIL (missing `UserRepository` / `PlaybackRepository`)

- [ ] **Step 3: Implement `UserRepository` and `PlaybackRepository`**

Create `crates/kadr-storage/src/repos/user_repo.rs`:
```rust
use deadpool_sqlite::Pool;
use rusqlite::params;
use kadr_core::models::{User, UserRole};
use crate::error::Result;

#[derive(Clone)]
pub struct UserRepository {
    pool: Pool,
}

impl UserRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: &User) -> Result<()> {
        let u = user.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let role_str = match u.role {
                UserRole::Admin => "admin",
                UserRole::Standard => "standard",
            };
            c.execute(
                "INSERT INTO users (id, username, pin_hash, role, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    username = excluded.username,
                    pin_hash = excluded.pin_hash,
                    role = excluded.role",
                params![u.id, u.username, u.pin_hash, role_str, u.created_at],
            )?;
            Ok(())
        }).await?
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<User>> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT id, username, pin_hash, role, created_at FROM users WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let username: String = row.get(1)?;
                let pin_hash: String = row.get(2)?;
                let role_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let role = match role_str.as_str() {
                    "admin" => UserRole::Admin,
                    _ => UserRole::Standard,
                };
                Ok(Some(User { id, username, pin_hash, role, created_at }))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        let username = username.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT id, username, pin_hash, role, created_at FROM users WHERE username = ?1")?;
            let mut rows = stmt.query(params![username])?;
            if let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let username: String = row.get(1)?;
                let pin_hash: String = row.get(2)?;
                let role_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let role = match role_str.as_str() {
                    "admin" => UserRole::Admin,
                    _ => UserRole::Standard,
                };
                Ok(Some(User { id, username, pin_hash, role, created_at }))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn list_all(&self) -> Result<Vec<User>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut stmt = c.prepare("SELECT id, username, pin_hash, role, created_at FROM users ORDER BY username ASC")?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let username: String = row.get(1)?;
                let pin_hash: String = row.get(2)?;
                let role_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let role = match role_str.as_str() {
                    "admin" => UserRole::Admin,
                    _ => UserRole::Standard,
                };
                Ok(User { id, username, pin_hash, role, created_at })
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        }).await?
    }

    pub async fn count(&self) -> Result<usize> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
            Ok(count as usize)
        }).await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute("DELETE FROM users WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        }).await?
    }
}
```

Create `crates/kadr-storage/src/repos/playback_repo.rs`:
```rust
use deadpool_sqlite::Pool;
use rusqlite::params;
use kadr_core::models::{PlaybackState, WatchState};
use crate::error::Result;

#[derive(Clone)]
pub struct PlaybackRepository {
    pool: Pool,
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

                Ok(Some(PlaybackState {
                    user_id,
                    media_item_id,
                    playback_position_seconds,
                    watch_state,
                    last_watched_at,
                    play_count,
                }))
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
            let (query, has_filter) = match watch_state {
                Some(state) => {
                    let state_str = match state {
                        WatchState::InProgress => "in_progress",
                        WatchState::Completed => "completed",
                        WatchState::Unwatched => "unwatched",
                    };
                    (
                        format!(
                            "SELECT user_id, media_item_id, playback_position_seconds, watch_state, last_watched_at, play_count
                             FROM user_playback_states
                             WHERE user_id = ?1 AND watch_state = '{}'
                             ORDER BY last_watched_at DESC
                             LIMIT ?2",
                            state_str
                        ),
                        true,
                    )
                }
                None => (
                    "SELECT user_id, media_item_id, playback_position_seconds, watch_state, last_watched_at, play_count
                     FROM user_playback_states
                     WHERE user_id = ?1
                     ORDER BY last_watched_at DESC
                     LIMIT ?2".to_string(),
                    false,
                ),
            };

            let mut stmt = c.prepare(&query)?;
            let rows = stmt.query_map(params![user_id, limit as i64], |row| {
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
            })?;

            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        }).await?
    }
}
```

Update `crates/kadr-storage/src/repos/mod.rs` and `crates/kadr-storage/src/lib.rs` to export `UserRepository` and `PlaybackRepository`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-storage --test user_and_playback_repos_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-storage
git commit -m "feat(storage): implement user and playback repositories"
```

---

### Task 3: PIN Validation, Argon2id Hashing & Token-Bucket Rate Limiter (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/auth/pin.rs`
- Create: `crates/kadr-server/src/auth/rate_limiter.rs`
- Create: `crates/kadr-server/src/auth/mod.rs`
- Modify: `crates/kadr-server/Cargo.toml`
- Test: `crates/kadr-server/tests/pin_and_rate_limit_test.rs`

**Interfaces:**
- Consumes: None
- Produces: `validate_pin`, `hash_pin`, `verify_pin`, `RateLimiter`, `RateLimitStatus`

- [ ] **Step 1: Write failing test for PIN hashing and rate limiting**

```rust
// crates/kadr-server/tests/pin_and_rate_limit_test.rs
use std::net::IpAddr;
use std::time::Duration;
use kadr_server::auth::pin::{hash_pin, validate_pin, verify_pin};
use kadr_server::auth::rate_limiter::{RateLimitStatus, RateLimiter};

#[test]
fn test_pin_validation_and_argon2_hashing() {
    assert!(validate_pin("1234").is_ok());
    assert!(validate_pin("0000").is_ok());
    assert!(validate_pin("123").is_err());
    assert!(validate_pin("12345").is_err());
    assert!(validate_pin("abcd").is_err());

    let hash = hash_pin("1234").unwrap();
    assert!(verify_pin("1234", &hash).unwrap());
    assert!(!verify_pin("9999", &hash).unwrap());
}

#[tokio::test]
async fn test_rate_limiter_lockout_after_five_attempts() {
    let limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let ip: IpAddr = "192.168.1.100".parse().unwrap();

    for _ in 0..4 {
        assert_eq!(limiter.check_attempt(&ip).await, RateLimitStatus::Allowed);
        limiter.record_failure(&ip).await;
    }

    assert_eq!(limiter.check_attempt(&ip).await, RateLimitStatus::Allowed);
    limiter.record_failure(&ip).await;

    // 5th failure triggers lockout
    match limiter.check_attempt(&ip).await {
        RateLimitStatus::LockedOut { retry_after_secs } => {
            assert!(retry_after_secs > 0 && retry_after_secs <= 300);
        }
        RateLimitStatus::Allowed => panic!("should be locked out"),
    }

    // Success on different IP works
    let other_ip: IpAddr = "192.168.1.200".parse().unwrap();
    assert_eq!(limiter.check_attempt(&other_ip).await, RateLimitStatus::Allowed);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test pin_and_rate_limit_test`
Expected: FAIL (missing `auth` module and dependencies)

- [ ] **Step 3: Implement PIN hashing and rate limiter**

Update `crates/kadr-server/Cargo.toml` with dependencies:
```toml
argon2 = "0.5"
rand = "0.8"
jsonwebtoken = "9.3"
axum = { version = "0.8", features = ["macros"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
tokio-util = { version = "0.7", features = ["io"] }
```

Create `crates/kadr-server/src/auth/pin.rs`:
```rust
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PinError {
    #[error("PIN must consist of exactly 4 numeric digits")]
    InvalidFormat,
    #[error("Argon2 hashing error: {0}")]
    HashError(String),
}

pub fn validate_pin(pin: &str) -> Result<(), PinError> {
    if pin.len() == 4 && pin.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err(PinError::InvalidFormat)
    }
}

pub fn hash_pin(pin: &str) -> Result<String, PinError> {
    validate_pin(pin)?;
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(pin.as_bytes(), &salt)
        .map_err(|e| PinError::HashError(e.to_string()))?
        .to_string();
    Ok(hash)
}

pub fn verify_pin(pin: &str, pin_hash: &str) -> Result<bool, PinError> {
    let parsed_hash = match PasswordHash::new(pin_hash) {
        Ok(h) => h,
        Err(_) => return Ok(false),
    };
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(pin.as_bytes(), &parsed_hash).is_ok())
}
```

Create `crates/kadr-server/src/auth/rate_limiter.rs`:
```rust
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitStatus {
    Allowed,
    LockedOut { retry_after_secs: u64 },
}

#[derive(Debug, Clone)]
struct AttemptRecord {
    failed_count: u32,
    first_failed_at: Instant,
    lockout_until: Option<Instant>,
}

#[derive(Clone)]
pub struct RateLimiter {
    max_attempts: u32,
    window_duration: Duration,
    lockout_duration: Duration,
    attempts: Arc<RwLock<HashMap<IpAddr, AttemptRecord>>>,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_duration: Duration, lockout_duration: Duration) -> Self {
        Self {
            max_attempts,
            window_duration,
            lockout_duration,
            attempts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn check_attempt(&self, ip: &IpAddr) -> RateLimitStatus {
        let now = Instant::now();
        let read = self.attempts.read().await;
        if let Some(record) = read.get(ip) {
            if let Some(lockout) = record.lockout_until {
                if now < lockout {
                    let remaining = lockout.duration_since(now).as_secs() + 1;
                    return RateLimitStatus::LockedOut { retry_after_secs: remaining };
                }
            }
        }
        RateLimitStatus::Allowed
    }

    pub async fn record_failure(&self, ip: &IpAddr) {
        let now = Instant::now();
        let mut write = self.attempts.write().await;
        let record = write.entry(*ip).or_insert(AttemptRecord {
            failed_count: 0,
            first_failed_at: now,
            lockout_until: None,
        });

        if now.duration_since(record.first_failed_at) > self.window_duration {
            record.failed_count = 1;
            record.first_failed_at = now;
            record.lockout_until = None;
        } else {
            record.failed_count += 1;
        }

        if record.failed_count >= self.max_attempts {
            record.lockout_until = Some(now + self.lockout_duration);
        }
    }

    pub async fn record_success(&self, ip: &IpAddr) {
        let mut write = self.attempts.write().await;
        write.remove(ip);
    }
}
```

Create `crates/kadr-server/src/auth/mod.rs`:
```rust
pub mod pin;
pub mod rate_limiter;

pub use pin::{hash_pin, validate_pin, verify_pin};
pub use rate_limiter::{RateLimitStatus, RateLimiter};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-server --test pin_and_rate_limit_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(server): implement 4-digit PIN verification and IP token-bucket rate limiter"
```

---

### Task 4: JWT Token Service & Axum Auth Extractors (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/auth/jwt.rs`
- Modify: `crates/kadr-server/src/auth/mod.rs`
- Test: `crates/kadr-server/tests/jwt_test.rs`

**Interfaces:**
- Consumes: `kadr-core::models::{User, UserRole, AuthClaims}`
- Produces: `JwtService`, `AuthUser`, `RequireAdmin`

- [ ] **Step 1: Write failing JWT encode/decode and extractor test**

```rust
// crates/kadr-server/tests/jwt_test.rs
use kadr_core::models::{User, UserRole};
use kadr_server::auth::jwt::JwtService;

#[test]
fn test_jwt_encode_and_decode() {
    let service = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let user = User {
        id: "u1".to_string(),
        username: "boody".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Admin,
        created_at: 1000,
    };

    let token = service.generate_token(&user).unwrap();
    let claims = service.verify_token(&token).unwrap();

    assert_eq!(claims.sub, "u1");
    assert_eq!(claims.username, "boody");
    assert_eq!(claims.role, UserRole::Admin);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test jwt_test`
Expected: FAIL (missing `jwt` module)

- [ ] **Step 3: Implement `JwtService` and Axum extractors**

Create `crates/kadr-server/src/auth/jwt.rs`:
```rust
use axum::{
    extract::{FromRequestParts, Query},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use kadr_core::models::{AuthClaims, User, UserRole};

#[derive(Clone)]
pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    ttl_seconds: usize,
}

impl JwtService {
    pub fn new(secret: &str, ttl_seconds: usize) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            ttl_seconds,
        }
    }

    pub fn generate_token(&self, user: &User) -> Result<String, jsonwebtoken::errors::Error> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize;
        let claims = AuthClaims {
            sub: user.id.clone(),
            username: user.username.clone(),
            role: user.role,
            exp: now + self.ttl_seconds,
            iat: now,
        };
        encode(&Header::default(), &claims, &self.encoding_key)
    }

    pub fn verify_token(&self, token: &str) -> Result<AuthClaims, jsonwebtoken::errors::Error> {
        let validation = Validation::default();
        let token_data = decode::<AuthClaims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: UserRole,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

pub struct RequireAdmin(pub AuthUser);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = if let Some(auth_header) = parts.headers.get("authorization") {
            if let Ok(header_str) = auth_header.to_str() {
                if let Some(bearer) = header_str.strip_prefix("Bearer ") {
                    Some(bearer.trim().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            // Check query param (?token=...)
            if let Some(query) = parts.uri.query() {
                let parsed: Result<Query<TokenQuery>, _> = Query::try_from_uri(&parts.uri);
                parsed.ok().and_then(|q| q.token.clone())
            } else {
                None
            }
        };

        let token = match token {
            Some(t) => t,
            None => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Missing authorization token" })),
                ).into_response());
            }
        };

        let jwt_service = match parts.extensions.get::<JwtService>() {
            Some(svc) => svc,
            None => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "JWT service not configured" })),
                ).into_response());
            }
        };

        match jwt_service.verify_token(&token) {
            Ok(claims) => Ok(AuthUser {
                id: claims.sub,
                username: claims.username,
                role: claims.role,
            }),
            Err(_) => Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid or expired token" })),
            ).into_response()),
        }
    }
}

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_user = AuthUser::from_request_parts(parts, state).await?;
        if auth_user.role == UserRole::Admin {
            Ok(RequireAdmin(auth_user))
        } else {
            Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "Admin privileges required" })),
            ).into_response())
        }
    }
}
```

Update `crates/kadr-server/src/auth/mod.rs` to export `jwt::{AuthUser, JwtService, RequireAdmin}`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-server --test jwt_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(server): implement JWT token service and Axum authentication extractors"
```

---

### Task 5: User Management & Authentication API Routes (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/auth_routes.rs`
- Create: `crates/kadr-server/src/api/user_routes.rs`
- Create: `crates/kadr-server/src/api/mod.rs`
- Modify: `crates/kadr-server/src/main.rs`
- Test: `crates/kadr-server/tests/auth_routes_test.rs`

**Interfaces:**
- Consumes: `kadr-storage::repos::UserRepository`, `kadr-server::auth::*`
- Produces: `/api/v1/users/profiles`, `/api/v1/auth/profile-pin`, `/api/v1/auth/me`, `/api/v1/users`

- [ ] **Step 1: Write failing Axum auth integration test**

```rust
// crates/kadr-server/tests/auth_routes_test.rs
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
use kadr_core::models::{User, UserRole};
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::auth::jwt::JwtService;
use kadr_server::api::create_router;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, MediaItemRepository, LibraryRepository};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_auth_profile_flow_and_rate_limiting() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };
    user_repo.create(&admin).await.unwrap();

    let jwt = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt.clone(),
        rate_limiter,
        Default::default(),
    );

    // 1. GET /api/v1/users/profiles
    let req = Request::builder().uri("/api/v1/users/profiles").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Bad PIN -> 401
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-1","pin":"9999"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 3. Good PIN -> 200 with JWT
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-1","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let token = json["token"].as_str().unwrap();

    // 4. GET /api/v1/auth/me with Bearer token
    let req = Request::builder()
        .uri("/api/v1/auth/me")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test auth_routes_test`
Expected: FAIL (missing `create_router`)

- [ ] **Step 3: Implement Auth and User API routes**

Create `crates/kadr-server/src/api/auth_routes.rs`:
```rust
use axum::{
    extract::{ConnectInfo, Extension, Json},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use kadr_core::models::UserRole;
use crate::auth::jwt::{AuthUser, JwtService};
use crate::auth::pin::verify_pin;
use crate::auth::rate_limiter::{RateLimitStatus, RateLimiter};
use kadr_storage::repos::UserRepository;

#[derive(Deserialize)]
pub struct PinAuthRequest {
    pub user_id: String,
    pub pin: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
}

pub async fn profile_pin_auth(
    Extension(user_repo): Extension<UserRepository>,
    Extension(jwt_svc): Extension<JwtService>,
    Extension(limiter): Extension<RateLimiter>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    Json(payload): Json<PinAuthRequest>,
) -> Response {
    let client_ip = connect_info
        .map(|ci| ci.0.ip())
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap());

    if let RateLimitStatus::LockedOut { retry_after_secs } = limiter.check_attempt(&client_ip).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", retry_after_secs.to_string())],
            Json(serde_json::json!({
                "error": "Too many failed attempts. Try again later.",
                "retry_after_seconds": retry_after_secs
            })),
        ).into_response();
    }

    let user = match user_repo.get_by_id(&payload.user_id).await {
        Ok(Some(u)) => u,
        _ => {
            limiter.record_failure(&client_ip).await;
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid user or PIN" })),
            ).into_response();
        }
    };

    match verify_pin(&payload.pin, &user.pin_hash) {
        Ok(true) => {
            limiter.record_success(&client_ip).await;
            match jwt_svc.generate_token(&user) {
                Ok(token) => (
                    StatusCode::OK,
                    Json(AuthResponse {
                        token,
                        user_id: user.id,
                        username: user.username,
                        role: user.role,
                    }),
                ).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to create session token" })),
                ).into_response(),
            }
        }
        _ => {
            limiter.record_failure(&client_ip).await;
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid user or PIN" })),
            ).into_response()
        }
    }
}

pub async fn get_current_user(auth_user: AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({
        "id": auth_user.id,
        "username": auth_user.username,
        "role": auth_user.role,
    }))
}
```

Create `crates/kadr-server/src/api/user_routes.rs`:
```rust
use axum::{
    extract::{Extension, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use kadr_core::models::{User, UserRole};
use crate::auth::jwt::RequireAdmin;
use crate::auth::pin::hash_pin;
use kadr_storage::repos::UserRepository;

#[derive(Serialize)]
pub struct ProfileCard {
    pub id: String,
    pub username: String,
    pub role: UserRole,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub pin: String,
    pub role: UserRole,
}

pub async fn list_profiles(Extension(user_repo): Extension<UserRepository>) -> impl IntoResponse {
    match user_repo.list_all().await {
        Ok(users) => {
            let cards: Vec<ProfileCard> = users
                .into_iter()
                .map(|u| ProfileCard {
                    id: u.id,
                    username: u.username,
                    role: u.role,
                })
                .collect();
            Json(cards).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to list profiles" })),
        ).into_response(),
    }
}

pub async fn create_user(
    _admin: RequireAdmin,
    Extension(user_repo): Extension<UserRepository>,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    let pin_hash = match hash_pin(&payload.pin) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            ).into_response();
        }
    };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let new_user = User {
        id: Uuid::new_v4().to_string(),
        username: payload.username,
        pin_hash,
        role: payload.role,
        created_at: now,
    };

    match user_repo.create(&new_user).await {
        Ok(_) => (
            StatusCode::CREATED,
            Json(ProfileCard {
                id: new_user.id,
                username: new_user.username,
                role: new_user.role,
            }),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response(),
    }
}
```

Create `crates/kadr-server/src/api/mod.rs` wiring the router:
```rust
pub mod auth_routes;
pub mod user_routes;

use axum::{
    routing::{get, post},
    Extension, Router,
};
use crate::auth::jwt::JwtService;
use crate::auth::rate_limiter::RateLimiter;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository};

pub fn create_router(
    user_repo: UserRepository,
    playback_repo: PlaybackRepository,
    media_repo: MediaItemRepository,
    lib_repo: LibraryRepository,
    jwt_svc: JwtService,
    limiter: RateLimiter,
    // Add additional state extensions as needed
) -> Router {
    Router::new()
        // Public profile list & auth
        .route("/api/v1/users/profiles", get(user_routes::list_profiles))
        .route("/api/v1/auth/profile-pin", post(auth_routes::profile_pin_auth))
        .route("/api/v1/auth/me", get(auth_routes::get_current_user))
        .route("/api/v1/users", post(user_routes::create_user))
        .layer(Extension(user_repo))
        .layer(Extension(playback_repo))
        .layer(Extension(media_repo))
        .layer(Extension(lib_repo))
        .layer(Extension(jwt_svc))
        .layer(Extension(limiter))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-server --test auth_routes_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(server): implement auth, user management API routes, and profile switching"
```

---

### Task 6: RFC 7233 Byte-Range Parser & HTTP 206 Streaming Handler (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/streaming/range.rs`
- Create: `crates/kadr-server/src/streaming/handler.rs`
- Create: `crates/kadr-server/src/streaming/mod.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Test: `crates/kadr-server/tests/streaming_test.rs`

**Interfaces:**
- Consumes: `MediaItemRepository`, `AuthUser`
- Produces: `parse_range_header`, `stream_media_item`, `GET /api/v1/stream/:item_id`

- [ ] **Step 1: Write failing Range request streaming test**

```rust
// crates/kadr-server/tests/streaming_test.rs
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;
use tower::ServiceExt;
use kadr_core::models::{MediaItem, MediaType, TechnicalInfo, MediaMetadata, User, UserRole};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::api::create_router;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, MediaItemRepository, LibraryRepository};
use std::time::Duration;

#[tokio::test]
async fn test_http_206_range_streaming() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let dir = tempdir().unwrap();
    let video_path = dir.path().join("sample.mp4");
    let mut file = File::create(&video_path).unwrap();
    // Write 1000 bytes (0..1000)
    let dummy_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    file.write_all(&dummy_data).unwrap();

    media_repo.upsert_batch(&[MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "Sample Movie".to_string(),
        original_title: None,
        release_year: Some(2021),
        added_at: 1000,
        file_path: video_path,
        file_name: "sample.mp4".to_string(),
        file_size: 1000,
        technical: TechnicalInfo {
            duration_seconds: 120,
            container: Some("mp4".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata::default(),
    }]).await.unwrap();

    let items = media_repo.list_by_library("lib1", 1, 0).await.unwrap();
    let item_id = items[0].id.unwrap();

    let jwt = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token = jwt.generate_token(&User {
        id: "u1".to_string(),
        username: "test".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    }).unwrap();

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
    );

    // 1. Full Stream (no Range) -> 200 OK
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-length").unwrap(), "1000");

    // 2. Partial Content (bytes=100-199) -> 206 Partial Content
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=100-199")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes 100-199/1000");
    assert_eq!(res.headers().get("content-length").unwrap(), "100");

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(body_bytes.len(), 100);
    assert_eq!(&body_bytes[..], &dummy_data[100..200]);

    // 3. Query token param (?token=...)
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}?token={}", item_id, token))
        .header("range", "bytes=0-9")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test streaming_test`
Expected: FAIL (missing streaming route)

- [ ] **Step 3: Implement RFC 7233 Range parser and streaming handler**

Create `crates/kadr-server/src/streaming/range.rs`:
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum ByteRange {
    Exact { start: u64, end: u64 },
    OpenEnded { start: u64 },
    Suffix { suffix: u64 },
}

pub fn parse_range_header(header: &str, total_size: u64) -> Option<(u64, u64)> {
    let header = header.trim();
    if !header.starts_with("bytes=") {
        return None;
    }
    let spec = &header["bytes=".len()..];
    // We only support single ranges per RFC 7233 standard direct play
    let range_part = spec.split(',').next()?.trim();

    if let Some(suffix_str) = range_part.strip_prefix('-') {
        let suffix: u64 = suffix_str.parse().ok()?;
        if suffix == 0 || total_size == 0 {
            return None;
        }
        let start = total_size.saturating_sub(suffix);
        let end = total_size - 1;
        Some((start, end))
    } else {
        let mut parts = range_part.split('-');
        let start_str = parts.next()?.trim();
        let end_str = parts.next()?.trim();

        let start: u64 = start_str.parse().ok()?;
        if start >= total_size {
            return None;
        }

        if end_str.is_empty() {
            Some((start, total_size - 1))
        } else {
            let end: u64 = end_str.parse().ok()?;
            if end < start {
                return None;
            }
            let clamped_end = end.min(total_size - 1);
            Some((start, clamped_end))
        }
    }
}
```

Create `crates/kadr-server/src/streaming/handler.rs`:
```rust
use axum::{
    body::Body,
    extract::{Extension, Path},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use std::io::SeekFrom;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use crate::auth::jwt::AuthUser;
use crate::streaming::range::parse_range_header;
use kadr_storage::repos::MediaItemRepository;

pub fn resolve_mime(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        _ => "application/octet-stream",
    }
}

pub async fn stream_media_item(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    headers: HeaderMap,
    Extension(media_repo): Extension<MediaItemRepository>,
) -> Response {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            ).into_response();
        }
    };

    let file = match File::open(&item.file_path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media file missing from disk" })),
            ).into_response();
        }
    };

    let total_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read file metadata" })),
            ).into_response();
        }
    };

    let container = item.technical.container.unwrap_or_else(|| {
        item.file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string()
    });
    let mime = resolve_mime(&container);

    if let Some(range_header) = headers.get(header::RANGE).and_then(|h| h.to_str().ok()) {
        if let Some((start, end)) = parse_range_header(range_header, total_size) {
            let length = end - start + 1;
            let mut file = file;
            if file.seek(SeekFrom::Start(start)).await.is_err() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Seek error" })),
                ).into_response();
            }

            let stream = ReaderStream::with_capacity(file.take(length), 64 * 1024);
            let body = Body::from_stream(stream);

            (
                StatusCode::PARTIAL_CONTENT,
                [
                    (header::CONTENT_TYPE, mime),
                    (header::ACCEPT_RANGES, "bytes"),
                    (header::CONTENT_LENGTH, length.to_string().as_str()),
                    (
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", start, end, total_size).as_str(),
                    ),
                ],
                body,
            ).into_response()
        } else {
            (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{}", total_size))],
                Body::empty(),
            ).into_response()
        }
    } else {
        let stream = ReaderStream::with_capacity(file, 64 * 1024);
        let body = Body::from_stream(stream);

        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime),
                (header::ACCEPT_RANGES, "bytes"),
                (header::CONTENT_LENGTH, total_size.to_string().as_str()),
            ],
            body,
        ).into_response()
    }
}
```

Create `crates/kadr-server/src/streaming/mod.rs` and attach `/api/v1/stream/:item_id` route in `crates/kadr-server/src/api/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-server --test streaming_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(server): implement RFC 7233 range parser and HTTP 206 streaming handler"
```

---

### Task 7: Playback Session Tracking, Heartbeat & Scrobble Synchronization (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/playback/session.rs`
- Create: `crates/kadr-server/src/playback/handler.rs`
- Create: `crates/kadr-server/src/playback/mod.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Test: `crates/kadr-server/tests/playback_test.rs`

**Interfaces:**
- Consumes: `PlaybackRepository`, `MediaItemRepository`, `AuthUser`
- Produces: `/api/v1/playback/sessions`, `/api/v1/playback/:session_id/progress`, `/api/v1/playback/states/:item_id`, `/api/v1/playback/continue-watching`

- [ ] **Step 1: Write failing playback session & scrobble integration test**

```rust
// crates/kadr-server/tests/playback_test.rs
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
use kadr_core::models::{MediaItem, MediaType, TechnicalInfo, MediaMetadata, User, UserRole, WatchState};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::api::create_router;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, MediaItemRepository, LibraryRepository};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_playback_session_lifecycle_and_scrobble() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    media_repo.upsert_batch(&[MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "Cairo Station".to_string(),
        original_title: None,
        release_year: Some(1958),
        added_at: 1000,
        file_path: PathBuf::from("/media/cairo.mp4"),
        file_name: "cairo.mp4".to_string(),
        file_size: 1000,
        technical: TechnicalInfo {
            duration_seconds: 5000,
            ..Default::default()
        },
        metadata: MediaMetadata::default(),
    }]).await.unwrap();

    let item_id = media_repo.list_by_library("lib1", 1, 0).await.unwrap()[0].id.unwrap();

    let jwt = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token = jwt.generate_token(&User {
        id: "u1".to_string(),
        username: "boody".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    }).unwrap();

    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
    );

    // 1. Create playback session
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = json["session_id"].as_str().unwrap();

    // 2. Send heartbeat (>60s) -> watch_state should become InProgress
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":150}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let state = playback_repo.get_state("u1", item_id).await.unwrap().unwrap();
    assert_eq!(state.watch_state, WatchState::InProgress);
    assert_eq!(state.playback_position_seconds, 150);

    // 3. Send heartbeat (>= 90% of 5000s = 4500s) -> Completed
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":4600}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let state = playback_repo.get_state("u1", item_id).await.unwrap().unwrap();
    assert_eq!(state.watch_state, WatchState::Completed);
    assert_eq!(state.play_count, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test playback_test`
Expected: FAIL (missing playback routes)

- [ ] **Step 3: Implement playback session tracking and scrobble handlers**

Create `crates/kadr-server/src/playback/session.rs`:
```rust
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use kadr_core::models::PlaybackSession;

#[derive(Clone)]
pub struct ActiveSession {
    pub session: PlaybackSession,
    pub last_heartbeat_instant: Instant,
}

#[derive(Default)]
pub struct SessionRegistry {
    sessions: RwLock<HashMap<String, ActiveSession>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, session: PlaybackSession) {
        let mut write = self.sessions.write().await;
        write.insert(
            session.session_id.clone(),
            ActiveSession {
                session,
                last_heartbeat_instant: Instant::now(),
            },
        );
    }

    pub async fn get(&self, session_id: &str) -> Option<PlaybackSession> {
        let read = self.sessions.read().await;
        read.get(session_id).map(|s| s.session.clone())
    }

    pub async fn update_progress(&self, session_id: &str, position: i64) -> Option<PlaybackSession> {
        let mut write = self.sessions.write().await;
        if let Some(active) = write.get_mut(session_id) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
            active.session.current_position_seconds = position;
            active.session.last_heartbeat_at = now;
            active.last_heartbeat_instant = Instant::now();
            Some(active.session.clone())
        } else {
            None
        }
    }

    pub async fn remove(&self, session_id: &str) -> Option<PlaybackSession> {
        let mut write = self.sessions.write().await;
        write.remove(session_id).map(|s| s.session)
    }

    pub async fn prune_stale(&self, max_idle: std::time::Duration) -> usize {
        let now = Instant::now();
        let mut write = self.sessions.write().await;
        let initial_len = write.len();
        write.retain(|_, s| now.duration_since(s.last_heartbeat_instant) < max_idle);
        initial_len - write.len()
    }
}
```

Create `crates/kadr-server/src/playback/handler.rs`:
```rust
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use kadr_core::models::{PlaybackSession, WatchState};
use crate::auth::jwt::AuthUser;
use crate::playback::session::SessionRegistry;
use kadr_storage::repos::{MediaItemRepository, PlaybackRepository};

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub media_item_id: i64,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub media_item_id: i64,
    pub duration_seconds: i64,
    pub resume_position_seconds: i64,
}

#[derive(Deserialize)]
pub struct ProgressHeartbeatRequest {
    pub position_seconds: i64,
}

pub fn evaluate_scrobble(position: i64, duration: i64) -> WatchState {
    if duration <= 0 {
        return WatchState::Unwatched;
    }
    let pct = position as f64 / duration as f64;
    if pct >= 0.90 {
        WatchState::Completed
    } else if position > 60 || pct > 0.02 {
        WatchState::InProgress
    } else {
        WatchState::Unwatched
    }
}

pub async fn create_session(
    auth_user: AuthUser,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    Extension(sessions): Extension<Arc<SessionRegistry>>,
    Json(payload): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let item = match media_repo.get_by_id(payload.media_item_id).await {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            ).into_response();
        }
    };

    let resume_pos = match playback_repo.get_state(&auth_user.id, payload.media_item_id).await {
        Ok(Some(s)) => {
            if s.watch_state == WatchState::Completed {
                0
            } else {
                s.playback_position_seconds
            }
        }
        _ => 0,
    };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let session = PlaybackSession {
        session_id: Uuid::new_v4().to_string(),
        user_id: auth_user.id,
        media_item_id: payload.media_item_id,
        duration_seconds: item.technical.duration_seconds,
        current_position_seconds: resume_pos,
        started_at: now,
        last_heartbeat_at: now,
    };

    sessions.insert(session.clone()).await;

    (
        StatusCode::CREATED,
        Json(SessionResponse {
            session_id: session.session_id,
            media_item_id: session.media_item_id,
            duration_seconds: session.duration_seconds,
            resume_position_seconds: resume_pos,
        }),
    ).into_response()
}

pub async fn progress_heartbeat(
    auth_user: AuthUser,
    Path(session_id): Path<String>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    Extension(sessions): Extension<Arc<SessionRegistry>>,
    Json(payload): Json<ProgressHeartbeatRequest>,
) -> impl IntoResponse {
    let session = match sessions.get(&session_id).await {
        Some(s) if s.user_id == auth_user.id => s,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Playback session not found or unauthorized" })),
            ).into_response();
        }
    };

    let watch_state = evaluate_scrobble(payload.position_seconds, session.duration_seconds);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

    if let Err(e) = playback_repo
        .upsert_progress(&auth_user.id, session.media_item_id, payload.position_seconds, watch_state, now)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response();
    }

    sessions.update_progress(&session_id, payload.position_seconds).await;

    StatusCode::OK.into_response()
}

pub async fn get_playback_state(
    auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> impl IntoResponse {
    match playback_repo.get_state(&auth_user.id, item_id).await {
        Ok(Some(s)) => Json(s).into_response(),
        Ok(None) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "user_id": auth_user.id,
                "media_item_id": item_id,
                "playback_position_seconds": 0,
                "watch_state": "unwatched",
                "last_watched_at": 0,
                "play_count": 0
            })),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response(),
    }
}

pub async fn list_continue_watching(
    auth_user: AuthUser,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> impl IntoResponse {
    match playback_repo.list_user_states(&auth_user.id, Some(WatchState::InProgress), 50).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response(),
    }
}

pub async fn close_session(
    auth_user: AuthUser,
    Path(session_id): Path<String>,
    Extension(sessions): Extension<Arc<SessionRegistry>>,
) -> impl IntoResponse {
    if let Some(session) = sessions.get(&session_id).await {
        if session.user_id == auth_user.id {
            sessions.remove(&session_id).await;
            return StatusCode::NO_CONTENT.into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}
```

Create `crates/kadr-server/src/playback/mod.rs` and attach routes to `crates/kadr-server/src/api/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-server --test playback_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(server): implement playback sessions, heartbeat progress, and scrobble synchronization"
```

---

### Task 8: Server Bootstrap Wiring & End-to-End Milestone 2 Integration Test

**Files:**
- Modify: `crates/kadr-server/src/main.rs`
- Create: `tests/e2e_streaming_auth_test.rs`
- Modify: `crates/kadr-server/Cargo.toml`
- Test: `tests/e2e_streaming_auth_test.rs`

**Interfaces:**
- Consumes: Complete workspace (`kadr-core`, `kadr-storage`, `kadr-ingest`, `kadr-server`)
- Produces: Live HTTP Axum server with Admin bootstrapping, range streaming, and session tracking

- [ ] **Step 1: Write comprehensive end-to-end integration test**

```rust
// tests/e2e_streaming_auth_test.rs
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::ServiceExt;
use kadr_core::models::{MediaItem, MediaType, TechnicalInfo, MediaMetadata, UserRole, WatchState};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::api::create_router;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, MediaItemRepository, LibraryRepository};

#[tokio::test]
async fn test_full_milestone_2_user_stream_and_scrobble_journey() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    // 1. Prepare dummy movie file on disk
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("film.mkv");
    let mut file = File::create(&video_path).unwrap();
    let sample_bytes: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    file.write_all(&sample_bytes).unwrap();

    media_repo.upsert_batch(&[MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "The Nightingale's Prayer".to_string(),
        original_title: Some("Doaa al-Karawan".to_string()),
        release_year: Some(1959),
        added_at: 1000,
        file_path: video_path,
        file_name: "film.mkv".to_string(),
        file_size: 5000,
        technical: TechnicalInfo {
            duration_seconds: 6000,
            container: Some("mkv".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata::default(),
    }]).await.unwrap();

    let media_item_id = media_repo.list_by_library("lib1", 1, 0).await.unwrap()[0].id.unwrap();

    // 2. Setup user and server router
    let admin_user = kadr_core::models::User {
        id: "admin-uid".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo.create(&admin_user).await.unwrap();

    let jwt_svc = JwtService::new("test-secret-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt_svc,
        rate_limiter,
        session_registry,
    );

    // 3. User lists profiles
    let req = Request::builder().uri("/api/v1/users/profiles").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. User logs in with 4-digit PIN
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-uid","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let auth_data: Value = serde_json::from_slice(&body).unwrap();
    let token = auth_data["token"].as_str().unwrap();

    // 5. User requests HTTP 206 range stream: bytes=1000-1999
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", media_item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=1000-1999")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes 1000-1999/5000");
    assert_eq!(res.headers().get("content-type").unwrap(), "video/x-matroska");

    let chunk = axum::body::to_bytes(res.into_body(), 2048).await.unwrap();
    assert_eq!(chunk.len(), 1000);
    assert_eq!(&chunk[..], &sample_bytes[1000..2000]);

    // 6. User starts playback session
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, media_item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let sess_data: Value = serde_json::from_slice(&body).unwrap();
    let session_id = sess_data["session_id"].as_str().unwrap();

    // 7. Send heartbeat ping (>60s) -> marks as InProgress
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":300}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 8. Verify Continue Watching returns this item
    let req = Request::builder()
        .uri("/api/v1/playback/continue-watching")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 2048).await.unwrap();
    let list: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["media_item_id"].as_i64().unwrap(), media_item_id);
    assert_eq!(list[0]["playback_position_seconds"].as_i64().unwrap(), 300);
}
```

- [ ] **Step 2: Connect router in `kadr-server::main` and start Axum listener**

Update `crates/kadr-server/src/main.rs`:
- Instantiate `JwtService` with secret from `data/jwt.secret` (or auto-generated).
- Instantiate `RateLimiter` (5 attempts / 300s window / 300s lockout).
- Instantiate `SessionRegistry` and spawn 30s maintenance task pruning idle sessions.
- Check `user_repo.count()`. If 0, seed initial admin user `admin` with PIN `1234` (or configured PIN).
- Assemble `create_router(...)`.
- Bind `tokio::net::TcpListener::bind(format!("{}:{}", config.server.host, config.server.port)).await`.
- Run `axum::serve(...)` with graceful shutdown signal.

- [ ] **Step 3: Run integration test and verify all pass**

Run: `cargo test --test e2e_streaming_auth_test`
Expected: PASS

- [ ] **Step 4: Verify workspace tests and linter**

Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server tests
git commit -m "feat(server): bind Axum HTTP router, admin bootstrap, and add Milestone 2 E2E test"
```
