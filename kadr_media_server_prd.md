# Product Requirements Document (PRD)

**Project Name:** Kadr (كادر)  
**System Type:** High-Performance, Minimalist Self-Hosted Media Server & Multi-Client Ecosystem  
**Target Architecture:** Rust Core + Embedded SQLite (WAL) + Web / TV / Mobile Clients  
**Document Status:** Final Draft / Ready for Implementation  
**Version:** 1.0.0  

---

## 1. Executive Summary & Problem Statement

### 1.1 Context
Mainstream self-hosted media server solutions (such as Plex, Jellyfin, and Emby) have evolved into heavy application stacks. Their runtime models introduce non-negligible idle memory consumption (250 MB to 1 GB+), depend on heavyweight runtimes (.NET or complex Python/Node wrappers), and enforce rigid, server-mandated UI home screens. End-users are forced into hardcoded landing screen structures ("Continue Watching", "Recently Added") with minimal ability to build dynamic, multi-factor collections tailored across different physical screen formats (10-ft living room UI, mobile touch screens, and desktop web interfaces).

### 1.2 Vision
**Kadr** (كادر — Arabic for cinematic *frame*) is a lightweight, zero-bloat, direct-play media server written in Rust with an embedded SQLite database. Kadr centers around a **Declarative Widget & Layout Engine** that treats home screen presentation as a dynamic canvas governed by customizable query filters. 

It provides:
- Single-digit millisecond screen hydration ($<5\text{ ms}$).
- Low idle system footprint ($\le 30\text{ MB}$ RSS memory).
- Household multi-tenancy secured by 4-digit PIN locks.
- An in-player online subtitle engine that automatically matches external subtitles by filename and release tag.

### 1.3 Key Performance Indicators (KPIs)
* **Idle System Footprint:** $\le 30\text{ MB}$ RSS memory usage.
* **Layout Hydration Latency:** $\le 5\text{ ms}$ for a full composite screen response (10+ complex widgets) on an indexed library of 50,000+ items.
* **Time-to-First-Byte (TTFB):** $<100\text{ ms}$ on local direct streams via HTTP 206 Partial Content range requests.
* **Binary Distribution:** Self-contained static binary ($\le 25\text{ MB}$) compiled against `musl`, zero external runtime dependencies.

---

## 2. User Personas & Core Use Cases

### 2.1 Personas
* **The Homelab Minimalist:** Demands efficient resource usage, explicit container/process isolation, predictable storage patterns, and low system overhead.
* **The Cinephile / Archivist:** Organizes media by director, country of origin, era, or runtime; wants targeted shelves (e.g., "1960s Egyptian Realism", "Studio Misr Restorations", "Gems under 90 minutes") rather than generic carousels.
* **The Living Room Household:** Multiple family members sharing a single Smart TV client; requires low-friction profile switching protected by a 4-digit PIN to maintain separate watch states.

### 2.2 User Stories & Use Cases
* **UC-01 (Custom TV Layout):** The user launches the living room TV app and is greeted by a custom layout: a Hero Billboard of uncompleted classic films, followed by a "Late Night Shorts (< 80 min)" shelf. The standard "Continue Watching" row is explicitly hidden on this device.
* **UC-02 (Secure Multi-User Switching):** A household member switches from the shared profile to their personal profile on the TV by entering their 4-digit PIN using the remote control.
* **UC-03 (In-Stream Subtitle Discovery):** While streaming an obscure release, the user encounters missing or out-of-sync subtitles. Without leaving playback, they trigger an in-player search. The system reads the active file's release string (`Film.1965.1080p.BluRay.x264-SceneGroup.mkv`), retrieves the exact matching Arabic subtitle from an online index, downloads it as a local sidecar, and hot-reloads it into the active video element.

---

## 3. Detailed Functional Specifications

### Module 1: Declarative Widget & Layout Engine

#### 1.1 Filter Abstract Syntax Tree (AST)
The server accepts an arbitrary JSON filter AST compiled into parameterized SQL using prepared statements. Raw user-supplied SQL is rejected.

* **Supported Attributes:**
  * `library_id` (Array/String): Target one or more libraries (`movies`, `shows`, `docs`, `home_videos`).
  * `item_type`: Scope by `movie`, `show`, `season`, or `episode`.
  * `added_at`, `released_at`, `last_watched_at`: Absolute epoch timestamps or relative sliding windows (e.g., `now - 30d`).
  * `duration_seconds`: Numerical comparisons (`gte`, `lte`, `between`).
  * `technical`: Resolution (`4k`, `1080p`), video codec (`hevc`, `av1`, `h264`), audio codec, audio channel layout, container format.
  * `metadata`: Country of origin (e.g., `Egypt`), primary audio language, director, writer, tags, studio.
  * `watch_state`: State filters (`unwatched`, `in_progress`, `completed`, dynamic percentage e.g., `progress between 0.10 and 0.90`).
* **Boolean Operators:** Supports recursive nesting of `AND`, `OR`, and `NOT` nodes.

#### 1.2 Widget Presentation Types
Each widget declares a presentation format:
* `billboard`: Single-item feature banner (title, high-res backdrop, synopsis, primary action).
* `carousel`: Standard horizontal scroll of media cards with lazy poster loading.
* `grid`: Responsive multi-row grid matrix.
* `dense_list`: Compact vertical rows showing progress bar, runtime, and title.

#### 1.3 Target-Specific Layout Matrices
* Layouts are decoupled from the query definitions. A `LayoutScreen` maps an ordered array of `widget_id`s to a specific `user_id` and `target_device` (`tv`, `mobile`, `web`, `all`).
* Support device-specific overrides (e.g., Living Room Apple TV vs. Bedroom Android TV).
* **Time- & Context-Aware Rules:** Layout configurations can declare active windows (e.g., `time_window: { start: "23:00", end: "04:00" }`), causing night-time shelves to automatically display during specific hours.

#### 1.4 Single-Roundtrip Screen Hydration
* The endpoint `GET /api/v1/screens/:target` must evaluate all associated widgets concurrently, hydrate poster URLs and user watch states, and return a single unified JSON payload.

---

### Module 2: Multi-User Tenancy & Access Control

#### 2.1 Profile Isolation
* Every database record tracking state (`user_playback_states`, `user_favorites`, `user_layouts`) must be partitioned by `user_id`.
* Scrobble states, playback progress, and play counts must remain strictly isolated between profiles.

#### 2.2 4-Digit Quick PIN Security
* **Storage:** PINs must be salted and hashed using Argon2id or scrypt.
* **Authentication Endpoint:** A lightweight auth route (`POST /api/v1/auth/profile-pin`) exchanging `{ user_id, pin }` for a scoped session JWT.
* **Security Constraints:**
  * Constant-time comparison to prevent timing side-channel attacks.
  * In-memory token bucket rate limiting per IP/client: Maximum 5 incorrect attempts before a progressive 5-minute lockout.

#### 2.3 Role-Based Access Control (RBAC)
* `Admin`: Full permissions across filesystem scanning, library creation, storage mapping, transcode parameters, and user account creation.
* `Standard`: Read access to assigned libraries, execution of widget queries, writing to personal watch states, and modifying personal layout screens.
* **Library Filtering:** Admin can assign whitelisted `library_id`s to specific users; query evaluations must enforce library ownership checks at the database query level.

---

### Module 3: Media Ingestion & Metadata Store

#### 3.1 Filesystem Monitoring
* Core engine binds to OS-level notifications (`inotify` on Linux, `kqueue` on macOS) via the `notify` crate.
* Avoid full scheduled disk polling loops. Ingestion queues must trigger within 500ms of file write completion.

#### 3.2 Metadata Ingestion Priority
1. **Local Sidecars:** Check for matching `.nfo` files and local artwork (`poster.jpg`, `cover.png`, `backdrop.jpg`).
2. **Deterministic Filename Extraction:** Parse titles, years, and technical tags directly from standard release names using regular expressions.
3. **Optional Scrapers:** Pluggable, non-blocking HTTP scrapers (TMDb) to populate missing overview text, director tags, and production country data. All metadata must be persisted locally in SQLite JSON structures to ensure fully offline operation once scraped.

---

### Module 4: Filename-Based Online Subtitle & Translation Engine

#### 4.1 In-Player Active Lookup
* The client player exposes an on-demand subtitle modal that hits `GET /api/v1/playback/:session_id/subtitles/search`.
* The server reads the active file's exact base name (e.g., `Bab.El-Hadid.1958.Restored.1080p.BluRay.x264-Ghareeb.mkv`).

#### 4.2 Release Parser & Multi-Provider Aggregation
* Extract search tokens: Title, Release Year, Source (`BluRay`, `WEB-DL`, `HDTV`), Resolution (`1080p`, `4k`), and Release Group.
* Query external providers in parallel:
  * OpenSubtitles API v2
  * SubDL / Subsource APIs
* Fallback: If zero results return for the full release string, fall back to querying by cleaned title + year.

#### 4.3 Sidecar Management & UTF-8 Normalization
* Selected subtitle files must be fetched by the server, converted to clean UTF-8 encoding (handling legacy `Windows-1256`, `ISO-8859-1`, etc.), and saved alongside the source video file as:  
  `{base_filename}.{language_code}.[forced|sdh].srt`
* The media server must notify the active playback session via SSE/WebSocket, prompting the video player to attach the new `<track>` without disrupting the video buffer.

#### 4.4 In-Player Synchronization & Translation Hook
* **Offset Control:** Client UI must provide an offset slider ($\pm 5.0\text{ s}$ in 50ms increments).
* **Machine Translation Fallback:** An optional hook allowing integration with a local LLM or translation API. If an Arabic subtitle is unavailable but an English subtitle is found, the system can translate cue text sequentially while maintaining timecode integrity, saving the output as `{base_filename}.ar.ai.srt`.

---

### Module 5: Playback & Streaming Architecture

#### 5.1 Native Direct Stream Engine
* Custom HTTP handler supporting standard byte-range requests (`RFC 7233` / `206 Partial Content`).
* Zero-copy file transmission leveraging async file handles (`tokio::fs::File`).
* Direct stream support for native formats (`MP4`, `MKV`, `WebM` containers containing H.264, HEVC, AV1 video and AAC, Opus audio).

#### 5.2 External Transcoding Architecture (Isolated Fallback)
* Direct play is always the default.
* If a target client lacks codec support, the server invokes an isolated child `ffmpeg` process streaming fragmented MP4 or HLS via `stdout` pipes.
* Hardware acceleration hooks (`VAAPI`, `NVENC`) configured via flags.

#### 5.3 Scrobble & Playback Synchronization
* Client emits heartbeat pings (`POST /api/v1/playback/:session_id/progress`) every 10 seconds.
* Server updates `user_playback_states` within a write transaction.
* Configurable thresholds:
  * Mark as "In Progress": $> 60\text{ seconds}$ or $> 2\%$ played.
  * Mark as "Completed": $\ge 90\%$ of total runtime.

---

### Module 6: Web Client & Management Studio

#### 6.1 Unified Single-Page Application
* **Cinema Player:** Clean HTML5 player with native subtitle rendering, audio track toggles, and responsive keyboard controls.
* **Layout Studio:** Visual drag-and-drop dashboard to assemble screen layouts, construct filter rules, and preview TV/Mobile/Web views side-by-side.
* **Server Health & Telemetry:** Real-time visibility into active streams, SQLite WAL file size, RSS memory usage, and background ingestion queues.

#### 6.2 Real-Time Event Bus
* Implement a persistent Server-Sent Events (SSE) channel (`/api/v1/events`).
* Broadcast events: `library:updated`, `layout:changed`, `subtitle:downloaded`, `session:synced`.

---

## 4. Technical Architecture & Database Design

### 4.1 System Component Topology

```
                    +-----------------------------------------------+
                    |                 Client Tier                   |
                    |   (TV Remote UI / Mobile App / Web Studio)    |
                    +-------+-------------------------------+-------+
                            | HTTP / SSE                    | HTTP 206 Direct Play
                            v                               v
+-------------------------------------------------------------------+
|                           Kadr Core                               |
|                         (Rust Binary)                             |
|                                                                   |
|  +-----------------------+             +-----------------------+  |
|  |    Axum HTTP Router   |             |   Stream Range Pipe   |  |
|  | (Auth, Layout, Scrob) |             | (Zero-Copy Async I/O) |  |
|  +-----------+-----------+             +-----------+-----------+  |
|              |                                     |              |
|  +-----------v-----------+                         |              |
|  |   AST Query Compiler  |                         |              |
|  +-----------+-----------+                         |              |
|              |                                     |              |
|  +-----------v-----------+                         |              |
|  |   SQLite Engine (WAL) |                         |              |
|  | (Embedded Connection) |                         |              |
|  +-----------------------+                         |              |
|                                                    |              |
|  +-----------------------+                         |              |
|  | Inotify / File Engine +----------+              |              |
|  +-----------------------+          |              |              |
+-------------------------------------+--------------+--------------+
                                      v              v
                          +-------------------------------+
                          |    Storage / Media Storage    |
                          |   (/movies, /shows, .srt)     |
                          +-------------------------------+
```

### 4.2 SQLite Schema Specification

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;

-- Users and Auth
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    pin_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'standard', -- 'admin', 'standard'
    created_at INTEGER NOT NULL
);

-- Media Catalog
CREATE TABLE media_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id TEXT NOT NULL,
    item_type TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    release_year INTEGER,
    duration_seconds INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    file_path TEXT UNIQUE NOT NULL,
    file_name TEXT NOT NULL,
    resolution TEXT,
    video_codec TEXT,
    audio_codec TEXT,
    audio_channels INTEGER,
    metadata JSON NOT NULL DEFAULT '{}'
);

-- Watch Progress
CREATE TABLE user_playback_states (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playback_position_seconds INTEGER NOT NULL DEFAULT 0,
    watch_state TEXT NOT NULL DEFAULT 'unwatched',
    last_watched_at INTEGER NOT NULL,
    play_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, media_item_id)
);

-- Widget Registry
CREATE TABLE widgets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_type TEXT NOT NULL,
    query_definition JSON NOT NULL,
    created_at INTEGER NOT NULL
);

-- Screen Layout Configurations
CREATE TABLE layout_screens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_device TEXT NOT NULL,
    name TEXT NOT NULL,
    widget_order JSON NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1
);

-- Indices for performance
CREATE INDEX idx_media_library_type ON media_items(library_id, item_type);
CREATE INDEX idx_media_added ON media_items(added_at DESC);
CREATE INDEX idx_playback_lookup ON user_playback_states(user_id, watch_state, last_watched_at DESC);
```

---

## 5. Non-Functional Requirements (NFRs)

* **Performance:** Maximum execution time for any compiled widget AST query must not exceed $10\text{ ms}$ on indexed datasets.
* **Reliability:** The SQLite database must recover cleanly without corruption during ungraceful server shutdowns (ensured via WAL mode and fsync settings).
* **Portability:** Cross-compile cleanly to `x86_64` and `aarch64` (Linux, macOS) with static musl toolchains.
* **Security:** All user endpoints authenticated via stateless HMAC/JWT tokens. Subtitle sidecar ingestion must validate file extensions and write strictly within the media item's parent directory to prevent directory traversal attacks.

---

## 6. Implementation Milestones

* **Milestone 1: Storage Engine & Core Ingestion**  
  Implement the SQLite migrations, embedded `rusqlite` connection pooling, and the `notify`-based file scanner.
* **Milestone 2: HTTP 206 Streaming & Authentication**  
  Axum-based direct file streamer supporting range requests, user management, and Argon2id 4-digit PIN verification.
* **Milestone 3: Widget AST Compiler & Screen API**  
  AST JSON data model, query compiler, and single-roundtrip screen hydration endpoint.
* **Milestone 4: Subtitle Fetcher & Dynamic Track Injection**  
  Filename tokenizer, OpenSubtitles/SubDL clients, UTF-8 conversion, and SSE hot-reloading notifications.
* **Milestone 5: Management Web App & Layout Studio**  
  Admin dashboard, interactive query builder, device preview pane, and embedded HTML5 video player.