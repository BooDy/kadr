# Design Specification: Kadr Milestone 2 — HTTP 206 Streaming Engine, PIN Auth & Playback Synchronization

**Document Status:** Approved  
**Date:** 2026-09-30  
**Target Milestone:** Milestone 2 (HTTP 206 Streaming & Authentication)  
**Author:** Pair Programming Agent & User  

---

## 1. System Overview & Scope

This specification covers **Milestone 2** of the Kadr media server ecosystem:
1. **Multi-User Tenancy & 4-Digit Quick PIN Security:** Salted Argon2id hashing, in-memory token bucket rate limiting (5 failed attempts $\to$ 5-minute lockout), constant-time PIN comparison, scoped session JWT generation, and profile switching endpoints.
2. **Native HTTP 206 Direct Streaming Engine:** Zero-copy async byte-range request processor conforming to RFC 7233 (`200 OK`, `206 Partial Content`, `416 Range Not Satisfiable`), bounded chunk streaming via `tokio::fs::File`, and MIME type resolution.
3. **Playback Session & Scrobble Synchronization Engine:** Stateful in-memory session registry (`session_id`), heartbeat ping handler (`POST /api/v1/playback/:session_id/progress`), configurable scrobble thresholds ($>60\text{s}$ or $>2\% \to \text{in\_progress}$, $\ge 90\% \to \text{completed}$), and atomic persistence in `user_playback_states`.

External transcoding (child `ffmpeg` processes) is deferred to a future streaming-refinement milestone, focusing this release strictly on high-performance direct stream delivery ($<100\text{ ms}$ TTFB, $\le 30\text{ MB}$ RSS).

---

## 2. Architecture & Subsystems

Milestone 2 builds directly on the Milestone 1 workspace:

```
crates/
├── kadr-core/                  # Core domain models (User, PlaybackState, PlaybackSession, AuthClaims)
├── kadr-storage/               # Migration 002 (user_playback_states), UserRepository, PlaybackRepository
├── kadr-ingest/                # (Milestone 1 ingestion pipeline remains unchanged)
└── kadr-server/                # Server binary integrating Axum HTTP router:
    ├── src/auth/               # Argon2id hasher, JWT service, in-memory IP rate limiter
    ├── src/streaming/          # RFC 7233 byte-range parser, async file streamer, MIME resolver
    ├── src/playback/           # In-memory SessionRegistry, scrobble evaluator, heartbeat handler
    └── src/api/                # Axum routes (/api/v1/auth, /api/v1/stream, /api/v1/playback, /api/v1/users)
```

---

## 3. Data Models (`kadr-core`)

All domain entities are strictly typed and serializable.

### 3.1 Types & Structures

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,                 // UUID v4
    pub username: String,
    pub pin_hash: String,           // Argon2id hash
    pub role: UserRole,
    pub created_at: i64,            // Unix epoch seconds
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
    pub sub: String,                // user_id
    pub username: String,
    pub role: UserRole,
    pub exp: usize,
    pub iat: usize,
}
```

---

## 4. SQLite Schema Evolution (`kadr-storage`)

### 4.1 Migration `002_playback_and_auth.sql`
Embedded via `rusqlite_migration` inside `kadr-storage`:

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

### 4.2 Repositories

#### `UserRepository`
* `create(user: &User) -> Result<()>`
* `get_by_id(id: &str) -> Result<Option<User>>`
* `get_by_username(username: &str) -> Result<Option<User>>`
* `list_all() -> Result<Vec<User>>`
* `count() -> Result<usize>`
* `delete(id: &str) -> Result<bool>`

#### `PlaybackRepository`
* `upsert_progress(user_id: &str, media_item_id: i64, position_seconds: i64, watch_state: WatchState, now: i64) -> Result<()>`
* `get_state(user_id: &str, media_item_id: i64) -> Result<Option<PlaybackState>>`
* `list_user_states(user_id: &str, watch_state: Option<WatchState>, limit: usize) -> Result<Vec<PlaybackState>>`

---

## 5. PIN Authentication, Rate Limiter & JWT (`kadr-server::auth`)

### 5.1 PIN Validation & Hashing
* PINs must be validated to contain exactly 4 decimal digits (`^[0-9]{4}$`).
* Hashes are computed using `argon2::Argon2` with default Argon2id parameters and random 16-byte cryptographic salts.
* Verification utilizes constant-time comparison to prevent timing side-channel attacks.

### 5.2 Token Bucket Rate Limiting
* `RateLimiter`: Thread-safe in-memory store tracking `(failed_attempts, lockout_until)` keyed by client `IpAddr`.
* **Rules:**
  * Increment counter on each incorrect PIN attempt.
  * When `failed_attempts >= 5` within 300 seconds, sets `lockout_until = now + 300s`.
  * Returns `HTTP 429 Too Many Requests` with `Retry-After: 300`.
  * Resets attempt count immediately upon successful authentication.

### 5.3 JWT Service & Axum Middleware
* Signs tokens using HMAC-SHA256 (`HS256`).
* Secret key: 32 bytes loaded from configuration or auto-generated at `data/jwt.secret`.
* Token lifetime defaults to 7 days.
* **Axum Extractor (`AuthUser`):**
  * Checks `Authorization: Bearer <token>` header first.
  * Falls back to `?token=<jwt>` query parameter (necessary for HTML5 `<video>` and `<audio>` stream tags).
  * Injects `AuthUser { id: String, username: String, role: UserRole }` into request handlers.
  * Emits `401 Unauthorized` on missing, expired, or invalid tokens.
* **Admin Guard (`RequireAdmin`):**
  * Rejects non-admin users with `403 Forbidden`.

### 5.4 Admin Bootstrap on Launch
* During server startup, if `UserRepository::count() == 0`:
  * Seeds default admin user (`admin`) using PIN configured in `kadr.toml` (or auto-generates a random 4-digit PIN and logs it securely via `tracing::info!`).

---

## 6. HTTP 206 Direct Stream Engine (`kadr-server::streaming`)

### 6.1 Range Processing (`RFC 7233`)
* **Full Stream (No Range Header):**
  * Returns `HTTP 200 OK`.
  * Headers: `Content-Length`, `Content-Type`, `Accept-Ranges: bytes`.
* **Byte Range (`bytes=start-end`, `bytes=start-`, `bytes=-suffix`):**
  * Normalizes and clamps `start` and `end` against total file size.
  * Returns `HTTP 206 Partial Content`.
  * Headers:
    * `Content-Range: bytes <start>-<end>/<total>`
    * `Content-Length: <end - start + 1>`
    * `Accept-Ranges: bytes`
    * `Content-Type: <mime>`
* **Invalid Range (`start >= total` or `start > end`):**
  * Returns `HTTP 416 Range Not Satisfiable`.
  * Header: `Content-Range: bytes */<total>`.

### 6.2 Zero-Copy Async Streaming Pipeline
* Uses `tokio::fs::File`.
* Seeks asynchronously to `start` via `seek(SeekFrom::Start(start))`.
* Restricts reading to `length` bytes via `.take(length)`.
* Pipes data in 64 KiB chunks via `tokio_util::io::ReaderStream` into `axum::body::Body::from_stream`.
* Memory usage remains strictly bounded ($\le 64\text{ KiB}$ buffer per stream handle), guaranteeing $<100\text{ ms}$ TTFB and preserving $\le 30\text{ MB}$ RSS.

---

## 7. Playback Sessions & Scrobble Synchronization (`kadr-server::playback`)

### 7.1 In-Memory Session Registry
* `SessionRegistry`: `Arc<RwLock<HashMap<String, PlaybackSession>>>`.
* `POST /api/v1/playback/sessions`:
  * Accepts `{ "media_item_id": 123 }`.
  * Generates a UUID v4 `session_id`.
  * Retrieves existing resume position from `user_playback_states`.
  * Stores session in memory and returns `{ session_id, media_item_id, duration_seconds, resume_position_seconds }`.
* Stale sessions without a heartbeat in $\ge 60\text{ seconds}$ are automatically purged by a 30-second interval maintenance task.
* `DELETE /api/v1/playback/sessions/:session_id`: Removes the session immediately on player stop.

### 7.2 Heartbeat & Scrobble Rules
* Client emits `POST /api/v1/playback/:session_id/progress` every 10 seconds with `{ "position_seconds": 450 }`.
* Progress percentage: $\text{progress\_pct} = \frac{\text{position\_seconds}}{\text{duration\_seconds}}$.
* **Threshold Evaluation:**
  1. $\text{progress\_pct} \ge 0.90 \implies \text{watch\_state} = \text{Completed}$, increment `play_count`.
  2. $\text{position\_seconds} > 60 \lor \text{progress\_pct} > 0.02 \implies \text{watch\_state} = \text{InProgress}$.
  3. Otherwise $\implies \text{watch\_state} = \text{Unwatched}$.
* State is persisted atomically in `user_playback_states`.

---

## 8. HTTP API Routes Summary

| Method | Path | Auth | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/users/profiles` | Public | List available profile cards for switching |
| `POST` | `/api/v1/auth/profile-pin` | Public (Rate-limited) | Exchange `{ user_id, pin }` for session JWT |
| `GET` | `/api/v1/auth/me` | User | Get current authenticated user profile |
| `POST` | `/api/v1/users` | Admin | Create a new user profile |
| `GET` | `/api/v1/stream/:item_id` | User (Header / Query) | HTTP 206 range stream of media file |
| `POST` | `/api/v1/playback/sessions` | User | Start playback session & get resume position |
| `POST` | `/api/v1/playback/:session_id/progress` | User | Send 10s heartbeat & update scrobble state |
| `GET` | `/api/v1/playback/states/:item_id` | User | Get user's saved playback position |
| `GET` | `/api/v1/playback/continue-watching` | User | List user's in-progress media items |
| `DELETE` | `/api/v1/playback/sessions/:session_id` | User | End active playback session |

---

## 9. Verification & Testing Strategy
* **Unit Tests:**
  * PIN validator and Argon2id constant-time hashing verification.
  * In-memory token bucket rate limiter (verifying 5 attempts trigger lockout and 300s expiration).
  * RFC 7233 Range parser (full range, open-ended, suffix, unsatisfiable ranges).
  * Scrobble threshold calculations ($>60\text{s}$, $>2\%$, $\ge 90\%$).
* **Integration Tests:**
  * Axum integration tests using `tower::ServiceExt::oneshot`:
    * Public profile lookup $\to$ PIN authentication $\to$ JWT exchange.
    * Rate limiter lockout after 5 consecutive bad PINs returning 429.
    * Authenticated stream endpoint with Range headers (`206 Partial Content` verifying exact slice byte contents).
    * Session creation $\to$ progress heartbeat $\to$ database verification of `user_playback_states` and continue-watching list.
