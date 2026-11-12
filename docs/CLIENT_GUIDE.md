# Kadr Media Server - Client Developer Guide

Welcome to the **Kadr Client Developer Guide**. This document is the definitive technical reference and implementation manual for building third-party client applications for Kadr, including Smart TV apps (Android TV, Google TV, Apple TV, Fire TV, LG webOS, Samsung Tizen), mobile apps (iOS, iPadOS, Android), desktop applications (macOS, Windows, Linux), and web clients.

---

## Table of Contents

1. [Overview & Protocol Conventions](#1-overview--protocol-conventions)
2. [Network Autodiscovery (`GET /api/v1/discovery`)](#2-network-autodiscovery-get-apiv1discovery)
3. [Authentication, User Profiles & Private Libraries](#3-authentication-user-profiles--private-libraries)
4. [Declarative Screen Layout Engine (AST)](#4-declarative-screen-layout-engine-ast)
5. [Media Details & Direct Folder Navigation](#5-media-details--direct-folder-navigation)
6. [Video Streaming, HLS Transcoding Fallback & Scrobbling](#6-video-streaming-hls-transcoding-fallback--scrobbling)
7. [Subtitles & Media Assets](#7-subtitles--media-assets)
8. [Hierarchical Folder Browsing & Real-Time Events (SSE)](#8-hierarchical-folder-browsing--real-time-events-sse)
9. [Playlists & Client Queue Playback](#9-playlists--client-queue-playback)
10. [Client Architecture Recipes & Best Practices](#10-client-architecture-recipes--best-practices)
11. [Error Handling & HTTP Status Matrix](#11-error-handling--http-status-matrix)

---

## 1. Overview & Protocol Conventions

Kadr is a high-performance personal media streaming server written in Rust with the Axum web framework and SQLite (WAL mode). Its architecture is designed for **instant direct-play streaming**, backed by an **on-demand HLS remuxing/transcoding fallback engine** for platforms (such as Samsung Tizen OS or web browsers) that encounter unsupported codecs or containers.

```mermaid
flowchart LR
    Client["Client App\n(TV / Mobile / Web)"] -- "1. Autodiscover" --> KadrServer["Kadr Media Server\n(Rust Axum Engine)"]
    Client -- "2. Authenticate & Profile PIN" --> KadrServer
    Client -- "3. Fetch Hydrated Screen AST" --> KadrServer
    Client -- "4a. Direct Play: HTTP 206 (Bytes)" --> Storage[("Disk Storage\nMKV / MP4 / WebM")]
    Client -- "4b. Fallback: HLS Master M3U8" --> Transcoder["HlsSessionManager\n(FFmpeg Remux / Transcode)"]
    Client -- "5. 10s Scrobble Heartbeat" --> KadrServer
    KadrServer -. "6. Server-Sent Events (SSE)" .-> Client
```

### Base URL & Versioning

All API endpoints are namespaced under `/api/v1`:

```
http://<server-ip>:8492/api/v1
```

- **Default Port**: `8492` (configurable via `config.toml` or CLI arguments).
- **Transport**: HTTP/1.1 or HTTP/2 over plain HTTP or TLS/HTTPS reverse proxies.
- **Payload Format**: `application/json; charset=utf-8` for all REST JSON payloads.

### Standard Request Headers

| Header Name | Type | Description |
| :--- | :--- | :--- |
| `Authorization` | String | Standard Bearer authentication: `Bearer <jwt_token>` |
| `X-Library-Unlock-Token` | String | Signed HMAC-SHA256 token unlocking private libraries |
| `X-Kadr-Unlocked` | String | Alternative header for unlocked libraries (comma-separated tokens supported) |
| `Accept` | String | Usually `application/json` or `text/event-stream` for SSE |
| `Range` | String | Standard HTTP byte-range header for video streaming: `bytes=0-` or `bytes=1048576-2097151` |

> [!TIP]
> **Streaming Authentication Fallback**: Native media players (such as HTML `<video>`, AVPlayer, or standard ExoPlayer datasources) cannot always inject custom HTTP headers into byte-range sub-requests. For streaming endpoints (`/api/v1/stream/{id}`), Kadr accepts authentication via query parameters:
> - `?token=<jwt_token>`
> - `?unlock_token=<unlock_token>` or `?unlocked=<unlock_token>`

### Direct-Play First & Universal Fallback Architecture

Kadr prioritizes **zero-copy direct-play streaming**, backed by an **on-demand HLS fallback pipeline**:
1. **Direct Stream (Default)**: The server acts as an ultra-fast, zero-copy, bounded I/O pipe serving raw container bytes using Tokio asynchronous streams (64 KB chunk size) and standard HTTP 206 range requests (`GET /api/v1/stream/{id}`).
2. **Client Capability Probing**: Client devices inspect item `technical` metadata (`container`, `video_codec`, `audio_codec`, `resolution`) returned by `GET /api/v1/items/{id}`.
3. **Universal HLS Fallback**: If the client lacks native support for the container or codecs (for example, Samsung Tizen TVs lacking DTS/TrueHD decoders or browsers lacking MKV support), the client requests the dynamic HLS master playlist (`GET /api/v1/media/{id}/hls/master.m3u8`).
   - **Smart Stream Copy**: When video is already compatible (e.g. H.264), the server copies the video track (`-c:v copy`) and only transcodes the audio to AAC, keeping server CPU usage near 0% and starting playback immediately.
   - **Full Transcode**: When video is unsupported (e.g. HEVC 10-bit or AV1 on older screens), the server transcodes video via `libx264` (or hardware acceleration if configured).

---

## 2. Network Autodiscovery (`GET /api/v1/discovery`)

Before displaying an IP entry screen, clients should perform local network discovery to locate running Kadr servers automatically.

### Endpoint Specification

- **Method & Route**: `GET /api/v1/discovery`
- **Authentication**: **None** (Publicly accessible)
- **Response Code**: `200 OK`

### Response Payload

```json
{
  "app": "kadr",
  "server_id": "8f3e2b1a-4c5d-6e7f-8a9b-0c1d2e3f4a5b",
  "name": "Living Room Server",
  "version": "0.1.1-alpha",
  "protocol_version": 1,
  "port": 8492,
  "setup_completed": true,
  "status": "online"
}
```

### Discovery Response Fields

| Field | Type | Description |
| :--- | :--- | :--- |
| `app` | String | Constant identifier: always `"kadr"`. |
| `server_id` | String | Persistent server UUID; remains stable across server reboots. |
| `name` | String | Human-readable server friendly name configured by the administrator. |
| `version` | String | Server semver release string (e.g. `"0.1.1-alpha"`). |
| `protocol_version` | Integer | Protocol integer version (`1`). Incremented for breaking protocol changes. |
| `port` | Integer | Port number bound by the HTTP server (e.g. `8492`). |
| `setup_completed` | Boolean | `false` if no administrator account exists yet; `true` once configured. |
| `status` | String | Server availability status: `"online"`. |

### Client Implementation Guidelines

```mermaid
sequenceDiagram
    participant Client as Client Application
    participant Subnet as Local Subnet (LAN)
    participant Server as Kadr Server (:8492)

    Client->>Subnet: Parallel HTTP GET /api/v1/discovery (e.g. 192.168.1.0/24)
    Server-->>Client: 200 OK (app: "kadr", server_id: "...", setup_completed: true)
    Client->>Client: Cache server_id & IP address
    alt setup_completed == false
        Client->>Client: Present Admin Setup Wizard
    else setup_completed == true
        Client->>Client: Navigate to Profile Select Screen
    end
```

1. **Subnet Probing**: On startup, client apps scan the active subnet on port `8492` by dispatching asynchronous `GET /api/v1/discovery` requests with a 1.5-second connection timeout.
2. **Server Identity Caching**: Store the returned `server_id` alongside the IP address. If the host IP shifts due to DHCP lease renewal, the client can match the existing saved library data by `server_id`.
3. **Setup Wizard Detection**: If `setup_completed` is `false`, redirect the user to the initial administrator account creation screen.

---

## 3. Authentication, User Profiles & Private Libraries

Kadr uses a TV-first user profile model designed for shared living rooms, paired with standard JSON Web Tokens (JWT) for session management and cryptographically signed HMAC-SHA256 tokens for unlocking private libraries.

```mermaid
sequenceDiagram
    participant User
    participant Client
    participant Server as Kadr API

    User->>Client: Launches App
    Client->>Server: GET /api/v1/auth/profiles
    Server-->>Client: 200 OK (Array of ProfileCard objects)
    Client->>User: Renders 10-foot profile avatar cards
    User->>Client: Selects profile & enters 4-digit PIN
    Client->>Server: POST /api/v1/auth/login {"user_id": "...", "pin": "1234"}
    alt PIN valid
        Server-->>Client: 200 OK {"token": "<jwt>", "user": {...}}
        Client->>Client: Persist token to secure storage
    else PIN invalid
        Server-->>Client: 401 Unauthorized {"error": "Invalid user or PIN"}
    else Rate limited (>5 attempts)
        Server-->>Client: 429 Too Many Requests (Retry-After: 30)
    end
```

### 3.1 Listing User Profiles

- **Method & Route**: `GET /api/v1/auth/profiles` (alias: `GET /api/v1/users/profiles`)
- **Authentication**: None
- **Response**: `200 OK`

```json
[
  {
    "id": "a1b2c3d4-0000-0000-0000-000000000001",
    "username": "Admin",
    "role": "admin",
    "has_pin": true,
    "avatar_color": "#E50914"
  },
  {
    "id": "b2c3d4e5-0000-0000-0000-000000000002",
    "username": "Kids",
    "role": "standard",
    "has_pin": true
  }
]
```

> [!NOTE]
> All users in Kadr have a PIN configured (`has_pin` is always `true`). Note that `avatar_color` is optional and omitted when unset; clients can deterministically generate profile avatar background gradients from the `username` string or profile index when not provided.

### 3.2 Logging In with Profile PIN

- **Method & Route**: `POST /api/v1/auth/login` (aliases: `POST /api/v1/auth/pin`, `POST /api/v1/auth/profile-pin`)
- **Authentication**: None

#### Request Body

```json
{
  "user_id": "a1b2c3d4-0000-0000-0000-000000000001",
  "pin": "1234"
}
```

#### Response (`200 OK`)

```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "user_id": "a1b2c3d4-0000-0000-0000-000000000001",
  "username": "Admin",
  "role": "admin",
  "user": {
    "id": "a1b2c3d4-0000-0000-0000-000000000001",
    "username": "Admin",
    "role": "admin"
  }
}
```

#### Rate Limiting & Lockout Protection

Kadr features an IP-based brute-force protector. After **5 consecutive failed PIN attempts**, the IP is locked out:

- **Status Code**: `429 Too Many Requests`
- **Response Header**: `Retry-After: 30`
- **Response Payload**:
  ```json
  {
    "error": "Too many failed attempts. Try again later.",
    "retry_after_seconds": 30
  }
  ```

### 3.3 Verifying Authentication Session

- **Method & Route**: `GET /api/v1/auth/me`
- **Headers**: `Authorization: Bearer <token>`
- **Response (`200 OK`)**:
  ```json
  {
    "id": "a1b2c3d4-0000-0000-0000-000000000001",
    "username": "Admin",
    "role": "admin"
  }
  ```

### 3.4 Private Libraries & Unlock Tokens

Libraries in Kadr can be designated as **Private** (`is_private: true`), secured with an independent 4-digit PIN. Private libraries are completely hidden from catalog queries, home screen carousels, and item search results until unlocked by the client.

#### Unlocking a Private Library

- **Method & Route**: `POST /api/v1/libraries/{id}/unlock`
- **Headers**: `Authorization: Bearer <token>`

##### Request Body
```json
{
  "pin": "9999"
}
```

##### Response (`200 OK`)
```json
{
  "library_id": "lib-vault-456",
  "token": "eyJsaWJyYXJ5X2lkIjoibGliLXZhdWx0LTQ1NiIsImV4cCI6MTcyODQwMDAwMH0.signature...",
  "unlock_token": "eyJsaWJyYXJ5X2lkIjoibGliLXZhdWx0LTQ1NiIsImV4cCI6MTcyODQwMDAwMH0.signature...",
  "expires_at": 1728400000
}
```

#### Using Unlock Tokens in Requests

When querying screens, item details, folder contents, or media streams belonging to an unlocked library, include the unlock token in the request:

1. **Header Format**:
   ```http
   X-Library-Unlock-Token: <token>
   ```
   *(or `X-Kadr-Unlocked: <token1>,<token2>` for multiple simultaneously unlocked libraries)*
2. **Query Parameter Format** (for streaming video & thumbnails):
   ```
   GET /api/v1/stream/123?unlock_token=<token>&token=<jwt_token>
   ```

#### Locked Library Access Denied

If an item or folder in a private library is accessed without a valid unlock token:
- **Status Code**: `403 Forbidden`
- **Response Body**:
  ```json
  {
    "error": "LIBRARY_LOCKED"
  }
  ```
Client applications should trap `LIBRARY_LOCKED` errors, prompt the user for the library PIN, execute the unlock request, and retry the pending action.

---

## 4. Declarative Screen Layout Engine (AST)

Kadr provides a **Server-Driven UI** layout engine. Instead of client applications hardcoding what shelves appear on the Home screen or Movies screen, the server returns an Abstract Syntax Tree (AST) specifying the layout, widget components, and bound data.

```mermaid
graph TD
    ScreenLayout["ScreenLayout (id: 'home', title: 'Home')"]
    ScreenLayout --> HeroNode["WidgetNode::HeroBanner\n(Spotlight candidate)"]
    ScreenLayout --> ContinueNode["WidgetNode::Carousel\n(macro: ContinueWatching)"]
    ScreenLayout --> RecentNode["WidgetNode::Carousel\n(macro: RecentlyAdded)"]
    ScreenLayout --> GridNode["WidgetNode::Grid\n(macro: TopRated, columns: 4)"]

    HeroNode -. hydrated .-> HeroData["CardViewModel\n(Backdrop, Title, Overview)"]
    ContinueNode -. hydrated .-> ContinueItems["Vec&lt;CardViewModel&gt;\n(Resume progress 0.45)"]
    RecentNode -. hydrated .-> RecentItems["Vec&lt;CardViewModel&gt;\n(Badge: 'NEW')"]
    GridNode -. hydrated .-> GridItems["Vec&lt;CardViewModel&gt;\n(Badge: '4K', Columns: 4)"]
```

### 4.1 Listing Screens

- **Method & Route**: `GET /api/v1/screens`
- **Headers**: `Authorization: Bearer <token>`
- **Response (`200 OK`)**:
  ```json
  [
    { "id": "home", "title": "Home" },
    { "id": "movies", "title": "Movies" },
    { "id": "shows", "title": "TV Shows" }
  ]
  ```

### 4.2 Fetching Hydrated Screen Layout

- **Method & Route**: `GET /api/v1/screens/{screen_id}`
- **Query Parameters**:
  - `unhydrated=true` *(optional)*: Returns the layout with empty widget items (useful for layout builders/studio). Omit or set to `false` for fully populated production layouts.
- **Headers**: `Authorization: Bearer <token>`, optional `X-Library-Unlock-Token: <token>`
- **Response (`200 OK`)**: Returns a `ScreenLayout` JSON object.

### 4.3 ScreenLayout Structure & Widget Node Types

```json
{
  "id": "home",
  "title": "Home",
  "widgets": [
    {
      "type": "hero_banner",
      "id": "widget-hero-1",
      "binding": {
        "macro_type": { "spotlight_item": { "item_id": null } },
        "limit": 1
      },
      "data": {
        "id": 101,
        "title": "Dune: Part Two",
        "subtitle": "2024 • 3840x2160",
        "poster_url": "/api/v1/artwork/101/poster",
        "backdrop_url": "/api/v1/artwork/101/backdrop",
        "media_type": "movie",
        "playback_progress": null,
        "rating": 8.6,
        "release_year": 2024,
        "badge": "4K"
      }
    },
    {
      "type": "carousel",
      "id": "widget-continue-watching",
      "title": "Continue Watching",
      "binding": {
        "macro_type": "continue_watching",
        "limit": 20
      },
      "items": [
        {
          "id": 42,
          "title": "Inception",
          "subtitle": "2010 • 1920x1080",
          "poster_url": "/api/v1/artwork/42/poster",
          "backdrop_url": "/api/v1/artwork/42/backdrop",
          "media_type": "movie",
          "playback_progress": 0.65,
          "rating": 8.8,
          "release_year": 2010,
          "badge": "RESUME"
        }
      ],
      "next_cursor": "/api/v1/widgets/widget-continue-watching/data?offset=20&limit=20&screen_id=home"
    },
    {
      "type": "grid",
      "id": "widget-top-rated-grid",
      "title": "Top Rated Movies",
      "binding": {
        "macro_type": "top_rated",
        "limit": 20
      },
      "columns": 4,
      "items": [ ... ],
      "next_cursor": "/api/v1/widgets/widget-top-rated-grid/data?offset=20&limit=20&screen_id=home",
      "total_count": 145
    }
  ]
}
```

### 4.4 AST Node Type Definitions

| Node `type` | Description | Hydrated Payload Field | Client Rendering Strategy |
| :--- | :--- | :--- | :--- |
| `hero_banner` | Featured spotlight title | `data: CardViewModel` | Full-width backdrop header with big action buttons ("Play", "Details"). |
| `carousel` | Horizontal scrolling shelf | `items: CardViewModel[]` | Single-row horizontal scrollable shelf with left/right pagination. |
| `grid` | Multi-column grid catalog | `items: CardViewModel[]`, `columns: number` | 2D responsive grid (e.g. 4 columns on TV, 2 on Mobile, 5 on Desktop). |
| `item_details` | Embedded single item details | `details: ItemDetailsPayload` | Detail view container with synopsis and episode lists. |

### 4.5 `QueryMacro` Data Source Bindings

Every `WidgetNode` carries a `binding` (`WidgetQueryBinding`) whose `macro_type` (`QueryMacro`, serialized with `#[serde(rename_all = "snake_case")]`) tells `WidgetResolver` how to hydrate items:

| `macro_type` JSON Representation | Description |
| :--- | :--- |
| `"continue_watching"` | Active user's in-progress items ordered by most recently watched. |
| `"recently_added"` | Recently ingested catalog items ordered by `added_at DESC`. |
| `"top_rated"` | Highest-rated catalog items ordered by rating. |
| `{"library_items": {"library_id": "<uuid>"}}` | Items scoped to a specific library UUID. |
| `{"genre_shelf": {"genre": "Sci-Fi"}}` | Items matching a specific genre tag. |
| `{"spotlight_item": {"item_id": null}}` | Featured hero candidate (or explicit `item_id`). |
| `{"item_details": {"item_id": 101}}` | Single hydrated item card by database ID. |
| `{"playlist_items": {"playlist_id": "<uuid>"}}` | Ordered items from a manual playlist (`Carousel` or `Grid` rail). |

#### Playlist Widget Binding (`playlist_items`)

Administrators can pin any playlist as a `carousel` or `grid` rail on `home` or custom screens:

```json
{
  "type": "carousel",
  "id": "widget-weekend-marathon",
  "title": "Sci-Fi Marathon",
  "binding": {
    "macro_type": {
      "playlist_items": {
        "playlist_id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d"
      }
    },
    "limit": 20
  },
  "items": [ ... ],
  "next_cursor": "20"
}
```

- **Cross-Library & Privacy-Isolated**: Because playlists can combine items from multiple libraries, `playlist_items` resolves across all libraries (unless `binding.filters.library_id` is explicitly set) while enforcing fail-closed PIN-locked library filtering via `X-Kadr-Unlocked` / `X-Library-Unlock-Token` and hydrating each `CardViewModel` with the **requesting viewer's** `PlaybackState`.
- **Pagination**: Supports standard incremental pagination via `GET /api/v1/widgets/{widget_id}/data?offset=<offset>&limit=<limit>&screen_id=<screen_id>`, slicing the ordered visible playlist items (`offset..min(offset + limit, total)`), returning `total_count` and advancing `next_cursor` while more items remain.
- **Graceful Empty Fallback**: If the referenced playlist has been deleted (`None`) or is a private playlist owned by another user (`owner_user_id != auth_user.id && !is_public`), `WidgetResolver` gracefully resolves an empty collection (`items: []`, `next_cursor: null`, `total_count: 0`) rather than failing `GET /api/v1/screens/{screen_id}` or `GET /api/v1/widgets/{widget_id}/data`.

### 4.6 The `CardViewModel` Structure

The core atomic UI component across all widgets is the `CardViewModel`:

```typescript
interface CardViewModel {
  id: number;                     // Media Item Database ID
  title: string;                  // Display title
  subtitle?: string;              // Normalized subtitle (e.g. "S02E04 - Episode Title" or "2024 • 4K UHD")
  poster_url?: string;            // Relative poster artwork endpoint: "/api/v1/artwork/{id}/poster"
  backdrop_url?: string;          // Relative backdrop endpoint: "/api/v1/artwork/{id}/backdrop"
  media_type: string;             // "movie" | "show" | "episode" | "season" | "home_videos"
  playback_progress?: number;     // Resume position percentage: float between 0.0 and 1.0
  rating?: number;                // Score (e.g. 8.4)
  release_year?: number;          // Release year (e.g. 2024)
  badge?: string;                 // Visual badge pill: "RESUME" | "NEW" | "4K"
  season?: number;                // Season number (for episodes)
  episode?: number;               // Episode number (for episodes)
}
```

#### Badge Calculation Rules
- **`RESUME`**: Calculated when the active user has an in-progress playback state (>60 seconds or >2% played, and <90% played).
- **`NEW`**: Calculated when the media file was ingested within the past 14 days.
- **`4K`**: Calculated when technical resolution metadata contains `4K` or `2160`.

### 4.7 Widget Item Pagination

When a user scrolls to the end of a carousel or catalog grid (including `playlist_items` rails), use the widget data endpoint:

- **Method & Route**: `GET /api/v1/widgets/{widget_id}/data`
- **Query Parameters**:
  - `offset`: Starting 0-based index (e.g. `20`)
  - `limit`: Number of items per batch (default: `20`)
  - `screen_id`: Optional screen scope (e.g. `home`, `movies`)
  - `sort`: Optional sort parameter
- **Headers**: `Authorization: Bearer <token>`, optional `X-Library-Unlock-Token: <token>` or `X-Kadr-Unlocked: <token1>,<token2>`

#### Response (`200 OK`)

```json
{
  "widget_id": "widget-top-rated-grid",
  "items": [ ... ],
  "next_cursor": "/api/v1/widgets/widget-top-rated-grid/data?offset=40&limit=20&screen_id=home",
  "total_count": 145
}
```

> [!TIP]
> If `next_cursor` is `null` or omitted, the widget has reached the end of the collection.

---

## 5. Media Details & Direct Folder Navigation

When a user clicks on any card, the client should query the unified item details endpoint.

### Endpoint Specification

- **Method & Route**: `GET /api/v1/items/{item_id}` (alias: `GET /api/v1/items/{item_id}/details`)
- **Headers**: `Authorization: Bearer <token>`, optional `X-Library-Unlock-Token: <token>`
- **Response**: `200 OK`

### 5.1 Movie Item Details Response Example

```json
{
  "card": {
    "id": 101,
    "title": "Dune: Part Two",
    "subtitle": "2024 • 3840x2160",
    "poster_url": "/api/v1/artwork/101/poster",
    "backdrop_url": "/api/v1/artwork/101/backdrop",
    "media_type": "movie",
    "playback_progress": 0.42,
    "rating": 8.6,
    "release_year": 2024,
    "badge": "RESUME"
  },
  "overview": "Paul Atreides unites with Chani and the Fremen while seeking revenge against the conspirators who destroyed his family.",
  "genres": ["Action", "Adventure", "Sci-Fi"],
  "duration_seconds": 9960,
  "technical": {
    "duration_seconds": 9960,
    "resolution": "3840x2160",
    "video_codec": "hevc",
    "audio_codec": "eac3",
    "audio_channels": 6,
    "container": "mkv"
  },
  "stream_url": "/api/v1/stream/101",
  "resume_position_seconds": 4183,
  "episodes": null,
  "library_id": "lib-movies-001",
  "folder_path": "Sci-Fi/Dune"
}
```

### 5.2 TV Show Details & Multi-Season Grouping

When the media item is a TV Show (`card.media_type === "show"`), the `episodes` field is populated with all child episodes:

```json
{
  "card": {
    "id": 205,
    "title": "Severance",
    "media_type": "show"
  },
  "episodes": [
    {
      "id": 206,
      "title": "Good News About Hell",
      "subtitle": "S01E01 - Severance",
      "season": 1,
      "episode": 1,
      "playback_progress": 1.0,
      "badge": null
    },
    {
      "id": 207,
      "title": "Half Loop",
      "subtitle": "S01E02 - Severance",
      "season": 1,
      "episode": 2,
      "playback_progress": 0.35,
      "badge": "RESUME"
    }
  ],
  "library_id": "lib-shows-001",
  "folder_path": "Severance"
}
```

#### Client UI Pattern for TV Shows:
1. Extract unique `season` numbers from `episodes` (e.g. `[1, 2]`).
2. Render a horizontal season tab bar (`Season 1`, `Season 2`).
3. Filter the displayed episode cards by the selected season.
4. Each episode card includes its individual `playback_progress` and can be clicked to start playback directly.

### 5.3 Direct Folder Navigation ("Browse Folder" Feature)

Notice the two folder attributes in every item details payload:
- `library_id`: ID of the library containing this item.
- `folder_path`: Relative folder path within the library root (e.g. `"Sci-Fi/Dune"`).

```mermaid
sequenceDiagram
    participant User
    participant DetailsModal as Item Details UI
    participant FolderView as Library Folder Explorer
    participant Server as Kadr API

    User->>DetailsModal: Clicks "Browse Folder" button
    DetailsModal->>FolderView: Navigate with library_id & folder_path
    FolderView->>Server: GET /api/v1/libraries/{library_id}/folders?path={folder_path}
    alt Library is public or already unlocked
        Server-->>FolderView: 200 OK (LibraryFolderResponse)
        FolderView->>User: Displays directory contents & companion files
    else Library is private & locked
        Server-->>FolderView: 403 Forbidden ("LIBRARY_LOCKED")
        FolderView->>User: Prompts 4-digit Library PIN
        User->>FolderView: Submits PIN
        FolderView->>Server: POST /api/v1/libraries/{id}/unlock
        Server-->>FolderView: 200 OK (unlock_token)
        FolderView->>Server: GET /api/v1/libraries/.../folders with X-Library-Unlock-Token
        Server-->>FolderView: 200 OK (LibraryFolderResponse)
    end
```

This feature allows seamless deep-linking: a user browsing the Home screen or Search results can immediately jump into the physical folder on disk to view related extras, soundtrack albums, or bonus featurettes.

---

## 6. Zero-Transcode Video Streaming & Scrobbling

Kadr streams video files directly via HTTP 206 Partial Content. The server does not burn CPU cycles on transcoding, guaranteeing instant seek times and maximum battery efficiency for client devices.

### 6.1 Direct-Play Streaming Endpoint

- **Method & Route**: `GET /api/v1/stream/{item_id}`
- **Authentication**: `Authorization: Bearer <token>` header or `?token=<token>` query parameter.
- **Unlock Token (Private Libraries)**: `X-Library-Unlock-Token: <token>` header or `?unlock_token=<token>` query parameter.

#### Sample Request

```http
GET /api/v1/stream/101 HTTP/1.1
Host: 192.168.1.50:8492
Authorization: Bearer eyJhbGciOiJIUzI1Ni...
Range: bytes=0-1048575
```

#### Sample Response (`206 Partial Content`)

```http
HTTP/1.1 206 Partial Content
Content-Type: video/x-matroska
Accept-Ranges: bytes
Content-Length: 1048576
Content-Range: bytes 0-1048575/8589934592
Cache-Control: no-cache

<binary video stream bytes>
```

#### MIME Types Returned
| Extension | Content-Type |
| :--- | :--- |
| `.mkv` | `video/x-matroska` |
| `.mp4`, `.m4v` | `video/mp4` |
| `.webm` | `video/webm` |
| `.mov` | `video/quicktime` |
| `.avi` | `video/x-msvideo` |
| `.mp3` | `audio/mpeg` |
| `.flac` | `audio/flac` |

---

### 6.2 Client Format Inspection: What to Look For

Before initiating playback, the client should inspect the `technical` metadata block returned by `GET /api/v1/items/{item_id}`:

```json
{
  "technical": {
    "duration_seconds": 9960,
    "resolution": "3840x2160",
    "video_codec": "hevc",
    "audio_codec": "dts",
    "audio_channels": 6,
    "container": "mkv"
  }
}
```

#### What Attributes to Check:
1. **`audio_codec`**: Check if the device has a native decoder for the audio format.
   - **Samsung Tizen OS**: **Lacks decoders for DTS, DTS-HD, and Dolby TrueHD**. Playing these directly results in silent video or player fatal error.
   - **Browsers (Chrome / Firefox)**: Lack support for DTS, TrueHD, and AC3/E-AC3 (Dolby Digital).
2. **`container`**: Check if the client's player pipeline supports the container file extension.
   - **Tizen HTML5 `<video>` / AVPlay**: Often fails on `.mkv` or `.avi` files, but natively plays HLS (`.m3u8`) and `.mp4`.
   - **Web Browsers**: Native HTML5 `<video>` supports `.mp4` and `.webm`, but **rejects `.mkv`**.
3. **`video_codec` & `resolution`**:
   - Check if the screen hardware can decode the resolution and profile (e.g. older 1080p Smart TVs cannot decode 4K HEVC 10-bit or AV1).

#### Client Decision Flowchart:

```mermaid
flowchart TD
    Start["User clicks Play on Media Item"] --> FetchDetails["Check item.technical metadata"]
    FetchDetails --> CheckAudio{"Is audio_codec supported?<br/>(e.g., Not DTS/TrueHD on Tizen)"}
    CheckAudio -- "No (Unsupported Audio)" --> FallbackHLS["Use HLS Fallback URL<br/>/api/v1/media/:id/hls/master.m3u8"]
    CheckAudio -- "Yes" --> CheckContainer{"Is container supported?<br/>(e.g., Not MKV on browser/Tizen)"}
    CheckContainer -- "No (Unsupported Container)" --> FallbackHLS
    CheckContainer -- "Yes" --> CheckVideo{"Is video_codec & resolution supported?"}
    CheckVideo -- "No" --> FallbackHLS
    CheckVideo -- "Yes (Direct Play Compatible)" --> DirectPlay["Use Direct Stream URL<br/>/api/v1/stream/:id"]

    FallbackHLS --> TranscodeMode{"Server Action"}
    TranscodeMode -->|"Video Compatible (e.g. H.264)"| SmartCopy["Smart Stream Copy (-c:v copy)<br/>Transcode only Audio to AAC<br/>0% CPU, Instant Start"]
    TranscodeMode -->|"Video Incompatible (e.g. HEVC 10-bit)"| FullTranscode["Transcode Video (libx264)<br/>Scale resolution if max_height requested"]
```

---

### 6.3 Universal HLS Transcoding & Remuxing Fallback

When direct play is not possible, the client points its player (AVPlay, Hls.js, or native HLS player) to Kadr's HLS master playlist endpoint.

#### 1. Master Playlist Request
- **Method & Route**: `GET /api/v1/media/{item_id}/hls/master.m3u8`
- **Authentication**: `Authorization: Bearer <token>` or `?token=<token>`
- **Unlock Token (Private Libraries)**: `X-Library-Unlock-Token: <token>` or `?unlock_token=<token>`

##### Query Parameters:
| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `video_codec` | String | `"h264"` | Target video codec required by the client (`"h264"` or `"hevc"`). |
| `audio_codec` | String | `"aac"` | Target audio codec required by the client (`"aac"`, `"ac3"`, `"eac3"`). |
| `max_height` | Integer | Source height | Optional max resolution height: `1080`, `720`, `480`. Automatically scales video down if source exceeds this. |
| `audio_track` | Integer | `0` | Stream index of the audio track to play. |
| `start_time` | Float | `0.0` | Initial seek offset in seconds for instant start at a resume position. |

##### Example Request (Tizen TV playing an MKV with DTS audio):
```http
GET /api/v1/media/101/hls/master.m3u8?video_codec=h264&audio_codec=aac&max_height=1080&token=eyJhbGci... HTTP/1.1
Host: 192.168.1.50:8492
```

##### Response (`200 OK`):
```http
HTTP/1.1 200 OK
Content-Type: application/vnd.apple.mpegurl
Cache-Control: no-cache

#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:5
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:4.000,
/api/v1/media/101/hls/sessions/e8b6238b-d72b-47e1-85b8-50fa973ff52c/segment_0000.ts
#EXTINF:4.000,
/api/v1/media/101/hls/sessions/e8b6238b-d72b-47e1-85b8-50fa973ff52c/segment_0001.ts
```

#### 2. Segment Delivery
- **Method & Route**: `GET /api/v1/media/{item_id}/hls/sessions/{session_id}/segment_{seq}.ts`
- **Response**: `200 OK` (`Content-Type: video/mp2t`)
- **Characteristics**:
  - Validates `session_id` strictly as a UUID and `seq` as a `u32` (eliminating path traversal).
  - Streams in bounded 64 KB chunks via Tokio `ReaderStream`.
  - Automatically updates the session's `last_accessed_at` timestamp to keep the session alive.

#### 3. Automatic Inactivity Cleanup
- The server runs a background reaper task every 15 seconds.
- If a client stops playing or closes the app, the session goes idle. When `now - last_accessed_at > 60 seconds`, the server automatically terminates the child `ffmpeg` process and deletes all temporary segment files from disk.

#### 4. Explicit Session Teardown (Optional)
When a user navigates away or stops playback, the client can proactively release server resources immediately:
- **Method & Route**: `DELETE /api/v1/media/{item_id}/hls/sessions/{session_id}`
- **Headers**: `Authorization: Bearer <token>`
- **Response**: `204 No Content`

---

### 6.4 Playback Session Lifecycle & Scrobble Synchronization

To maintain resume positions across devices and synchronize Continue Watching carousels, clients implement a three-step session tracking protocol:

```mermaid
sequenceDiagram
    participant Player as Client Video Player
    participant Kadr as Kadr Server
    participant SSE as Other Devices (SSE)

    Player->>Kadr: 1. POST /api/v1/playback/sessions {"media_item_id": 101}
    Kadr-->>Player: 201 Created {"session_id": "sess-xyz", "resume_position_seconds": 4183}
    Player->>Player: Seek player to 4183s & begin playback

    loop Every 10 Seconds
        Player->>Kadr: 2. POST /api/v1/playback/sess-xyz/progress {"position_seconds": 4193}
        Kadr-->>Player: 200 OK
        Kadr-.->SSE: Broadcast SSE event: "session:synced"
    end

    User->>Player: Exits player / Finishes video
    Player->>Kadr: 3. DELETE /api/v1/playback/sessions/sess-xyz
    Kadr-->>Player: 204 No Content
```

#### Step 1: Starting a Session

Call when the player initializes:
- **Route**: `POST /api/v1/playback/sessions`
- **Headers**: `Authorization: Bearer <token>`
- **Request Body**:
  ```json
  {
    "media_item_id": 101
  }
  ```
- **Response (`201 Created`)**:
  ```json
  {
    "session_id": "d3b07384-d113-4f90-bc0f-90e8a719ef58",
    "media_item_id": 101,
    "duration_seconds": 9960,
    "resume_position_seconds": 4183
  }
  ```
Client players should immediately seek to `resume_position_seconds` (if `resume_position_seconds > 0`).

#### Step 2: Periodic 10-Second Heartbeat

Dispatch every **10 seconds** while video playback is actively advancing:
- **Route**: `POST /api/v1/playback/{session_id}/progress`
- **Headers**: `Authorization: Bearer <token>`
- **Request Body**:
  ```json
  {
    "position_seconds": 4193
  }
  ```
- **Response**: `200 OK`

##### Automatic Server-Side Scrobble Evaluation Rules:
The server evaluates watch state according to strict threshold rules:
1. **`Unwatched`**: Position < 60 seconds and played < 2% of total duration.
2. **`InProgress`**: Position > 60 seconds or played > 2% of total duration. The item appears in `Continue Watching` shelves with an accurate resume progress bar.
3. **`Completed`**: Position reaches **90% or higher** of total duration. Play count increments by 1, and resume position resets to 0.

#### Step 3: Ending a Session

Call when the user closes the player or playback finishes:
- **Route**: `DELETE /api/v1/playback/sessions/{session_id}` (or `DELETE /api/v1/playback/{session_id}`)
- **Headers**: `Authorization: Bearer <token>`
- **Response**: `204 No Content`

#### Step 4: Querying Playback State & Continue Watching

- **Single Item Playback State**: `GET /api/v1/playback/states/{item_id}`
  ```json
  {
    "user_id": "a1b2c3d4-...",
    "media_item_id": 101,
    "playback_position_seconds": 4193,
    "watch_state": "in_progress",
    "last_watched_at": 1728399500,
    "play_count": 0
  }
  ```
- **All Continue Watching Items**: `GET /api/v1/playback/continue-watching`
  Returns up to 50 items currently in the `in_progress` state for the logged-in user.

---

## 7. Subtitles & Media Assets

Kadr provides first-class subtitle delivery with on-the-fly WebVTT conversion, disk caching, in-player online subtitle search via OpenSubtitles, and high-resolution artwork delivery.

### 7.1 Subtitle Discovery

- **Method & Route**: `GET /api/v1/items/{item_id}/subtitles`
- **Headers**: `Authorization: Bearer <token>`
- **Response (`200 OK`)**:

```json
[
  {
    "id": 12,
    "media_item_id": 101,
    "source": "sidecar",
    "language": "en",
    "title": "English [Full]",
    "format": "srt",
    "is_default": true,
    "is_forced": false,
    "stream_url": "/api/v1/subtitles/12/stream.vtt"
  },
  {
    "id": 13,
    "media_item_id": 101,
    "source": "downloaded",
    "language": "ara",
    "title": "Arabic",
    "format": "vtt",
    "is_default": false,
    "is_forced": false,
    "stream_url": "/api/v1/subtitles/13/stream.vtt"
  }
]
```

### 7.2 WebVTT Subtitle Streaming

Clients consume subtitles by passing `stream_url` directly to player text track APIs:

- **Method & Route**: `GET /api/v1/subtitles/{subtitle_id}/stream.vtt`
- **Authentication**: Public / query token fallback supported (`?token=<jwt>`).
- **Features**:
  - Automatically converts SubRip (`.srt`) files to WebVTT (`.vtt`) in real time.
  - Caches converted files on disk for instant re-delivery.
  - Sets standard headers:
    - `Content-Type: text/vtt; charset=utf-8`
    - `Cache-Control: public, max-age=86400`
    - `Accept-Ranges: bytes`

### 7.3 In-Player Live Subtitle Search & Download

If the user needs a subtitle track not present on disk, clients can search OpenSubtitles directly while remaining inside the playback interface.

#### 1. Search OpenSubtitles

- **Method & Route**: `GET /api/v1/subtitles/{item_id}/search?languages=en,ar,es`
- **Headers**: `Authorization: Bearer <token>`
- **Response (`200 OK`)**:
  ```json
  {
    "configured": true,
    "matches": [
      {
        "id": "os-sub-881",
        "file_id": "19532891",
        "language": "en",
        "format": "srt",
        "release": "Dune.Part.Two.2024.1080p.WEBRip",
        "download_count": 2841,
        "rating": 9.2
      }
    ]
  }
  ```

#### 2. Download Selected Subtitle

- **Method & Route**: `POST /api/v1/subtitles/{item_id}/download`
- **Headers**: `Authorization: Bearer <token>`
- **Request Body**:
  ```json
  {
    "file_id": "19532891",
    "language": "en",
    "title": "English (WEBRip)",
    "is_forced": false
  }
  ```
- **Response (`201 Created`)**:
  Returns the registered `SubtitleTrackResponse` object with its new `stream_url`. The server simultaneously emits a `subtitle:downloaded` event over SSE so other active players update their track lists.

---

### 7.4 Media Artwork & Video Thumbnails

| Media Type | Endpoint | Description |
| :--- | :--- | :--- |
| **Movie / Episode Poster** | `GET /api/v1/artwork/{item_id}/poster` | Returns indexed poster image. If no poster exists on disk, **automatically extracts a video frame thumbnail** at 10% duration. |
| **Backdrop Artwork** | `GET /api/v1/artwork/{item_id}/backdrop` | Returns high-resolution 16:9 fanart backdrop image. |
| **Unindexed Video Thumbnail** | `GET /api/v1/libraries/{id}/thumbnail?path={rel_path}` | On-demand video thumbnail extraction for files in library folders. |
| **Raw Library Image** | `GET /api/v1/libraries/{id}/image?path={rel_path}` | Sandboxed streaming of high-resolution stills, album art, or photo files. |

All image routes include appropriate `Content-Type` headers (`image/jpeg`, `image/png`, `image/webp`, `image/avif`) and security sandboxing against path traversal attacks.

---

## 8. Hierarchical Folder Browsing & Real-Time Events (SSE)

### 8.1 Browsing Library Folders

For users who organize media in customized directory hierarchies (e.g. `Movies/Sci-Fi/Franchise/`), Kadr provides full directory navigation.

- **Method & Route**: `GET /api/v1/libraries/{id}/folders`
- **Query Parameters**:
  - `path`: Subdirectory path relative to the library root (e.g. `path=Sci-Fi/Dune`). Omit for root.
- **Headers**: `Authorization: Bearer <token>`, optional `X-Library-Unlock-Token: <token>`
- **Response Code**: `200 OK`

#### Response Payload (`LibraryFolderResponse`)

```json
{
  "library_id": "lib-movies-001",
  "library_name": "Movies",
  "current_path": "Sci-Fi/Dune",
  "parent_path": "Sci-Fi",
  "breadcrumbs": [
    { "name": "Movies", "path": "" },
    { "name": "Sci-Fi", "path": "Sci-Fi" },
    { "name": "Dune", "path": "Sci-Fi/Dune" }
  ],
  "directories": [
    {
      "name": "Behind The Scenes",
      "path": "Sci-Fi/Dune/Behind The Scenes",
      "item_count": 3
    }
  ],
  "items": [
    {
      "id": 101,
      "title": "Dune: Part Two",
      "subtitle": "2024 • 3840x2160",
      "poster_url": "/api/v1/artwork/101/poster",
      "backdrop_url": "/api/v1/artwork/101/backdrop",
      "media_type": "movie",
      "playback_progress": 0.42,
      "badge": "RESUME"
    }
  ],
  "images": [
    {
      "name": "poster_custom.jpg",
      "path": "Sci-Fi/Dune/poster_custom.jpg",
      "url": "/api/v1/libraries/lib-movies-001/image?path=Sci-Fi/Dune/poster_custom.jpg",
      "size_bytes": 1048576
    }
  ]
}
```

> [!TIP]
> Items returned in `items` are fully normalized `CardViewModel` objects enriched with metadata, resolution badges, and current user playback progress bars!

---

### 8.2 Real-Time Server-Sent Events (SSE)

Clients can subscribe to server notifications over a single persistent SSE stream.

- **Method & Route**: `GET /api/v1/events`
- **Headers**: `Accept: text/event-stream`
- **Heartbeat**: The server emits an empty comment / `: ping` every **15 seconds** to prevent intermediate router or reverse-proxy timeouts.

#### Event Stream Format

```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive

: ping

event: library:updated
data: {"type":"library:updated","payload":{"library_id":"lib-movies-001","item_count":145,"timestamp":1728399500}}

event: session:synced
data: {"type":"session:synced","payload":{"session_id":"d3b07384-...","item_id":101,"user_id":"usr-1","position_seconds":4193,"timestamp":1728399510}}

event: subtitle:downloaded
data: {"type":"subtitle:downloaded","payload":{"item_id":101,"subtitle_id":13,"language":"ara","timestamp":1728399520}}

event: layout:changed
data: {"type":"layout:changed","payload":{"screen_id":"home","timestamp":1728399530}}

event: playlist:updated
data: {"type":"playlist:updated","payload":{"playlist_id":"6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d","user_id":"u-123","action":"items_changed","timestamp":1762942100}}
```

#### Event Catalog

| Event Name | Payload Structure | Client Action |
| :--- | :--- | :--- |
| `library:updated` | `{"library_id": string, "item_count": number, "timestamp": number}` | Refresh library lists and catalog grids. |
| `session:synced` | `{"session_id": string, "item_id": number, "user_id": string, "position_seconds": number, "timestamp": number}` | Synchronize Continue Watching carousels across other screens without full page reloads. |
| `subtitle:downloaded` | `{"item_id": number, "subtitle_id": number, "language": string, "timestamp": number}` | Refresh in-player subtitle track selection menu. |
| `layout:changed` | `{"screen_id": string, "timestamp": number}` | Invalidate cached screen AST layout and re-render widgets. |
| `playlist:updated` | `{"playlist_id": string, "user_id": string, "action": string, "timestamp": number}` | Refresh playlist lists, active playlist detail views, or `playlist_items` AST rails matching `playlist_id`. |
| `system:telemetry` | `{"active_sessions_count": number, "rss_memory_bytes": number, ...}` | Admin dashboard live resource meter update. |

#### `playlist:updated` Payload & Privacy Design

```json
{
  "type": "playlist:updated",
  "payload": {
    "playlist_id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
    "user_id": "u-123",
    "action": "items_changed",
    "timestamp": 1762942100
  }
}
```

- `action` is one of `"created"`, `"updated"`, `"deleted"`, or `"items_changed"`.
- **Intentional Metadata Omission**: Because `GET /api/v1/events` is an unauthenticated broadcast stream shared by all connected clients, `playlist:updated` intentionally omits playlist names, descriptions, media item IDs, item titles, and poster URLs. Clients receive only the opaque `playlist_id` invalidation signal and re-fetch `GET /api/v1/playlists` or `GET /api/v1/playlists/{id}` using their authenticated `Authorization` and `X-Kadr-Unlocked` / `X-Library-Unlock-Token` headers.

---

## 9. Playlists & Client Queue Playback

Kadr supports manual ordered playlists with multi-user server sharing, 1-step Create & Add, batch TV Show / Season / sandboxed Folder expansion, fail-closed PIN-locked library isolation, and client-orchestrated multi-item queue playback.

### 9.1 Wire Models, Set-Membership & Multi-User Ownership

- **Authentication & Unlock Headers**: Every `/api/v1/playlists/*` endpoint requires `Authorization: Bearer <jwt_token>` (`401 Unauthorized` when missing or expired) and inspects `X-Kadr-Unlocked` / `X-Library-Unlock-Token` (or `?unlocked=` / `?unlock_token=`) to determine which PIN-locked private libraries are unlocked for the request.
- **Set-Membership & Dense Ordering**: Each playlist enforces set-membership per `(playlist_id, media_item_id)` (`PRIMARY KEY (playlist_id, media_item_id)`). Duplicate additions are idempotently skipped (`INSERT OR IGNORE`), and stored items maintain dense 0-based positions (`0..N-1`).
- **Client-Orchestrated Queue State**: Kadr persists playlist definitions on the server and synchronizes per-item watch progress via `user_playback_states`, while active playback queue progression (`currentIndex`, shuffle order, next/previous track transitions) runs in client memory.

#### Core Playlist Wire Types

```typescript
interface PlaylistSummary {
  id: string;                        // Playlist UUID v4
  name: string;                      // 1..=120 trimmed UTF-8 characters
  description?: string;              // <= 500 trimmed UTF-8 characters; omitted when null/empty
  owner_user_id: string;             // Owner profile UUID
  owner_username: string;            // Owner display username
  is_public: boolean;                // true = shared with all authenticated profiles on the server
  is_editable: boolean;              // true when caller is the owner or has role "admin"
  visible_item_count: number;        // Count of items currently visible to the caller
  locked_item_count: number;         // Aggregate count of items hidden in locked private libraries
  total_duration_seconds: number;    // Sum of duration_seconds across visible items only
  cover_poster_urls: string[];       // Up to 4 "/api/v1/artwork/{id}/poster" URLs from visible items
  created_at: number;                // Unix epoch seconds
  updated_at: number;                // Unix epoch seconds
  contains_queried_item?: boolean;   // Present ONLY when ?contains_item_id=<id> is supplied
}

interface PlaylistItemView {
  position: number;                  // Normalized 0-based index (0..items.length - 1) across visible items
  added_at: number;                  // Unix epoch seconds when added to playlist
  duration_seconds: number;          // Item runtime in seconds (>= 0)
  resume_position_seconds?: number;  // Present ONLY when viewer's watch_state == "in_progress" && > 0
  card: CardViewModel;               // Nested CardViewModel hydrated with the viewer's PlaybackState
}

interface PlaylistDetail {
  summary: PlaylistSummary;
  items: PlaylistItemView[];
}
```

> [!NOTE]
> **Viewer-Scoped Playback State**: On shared public playlists (`is_public: true`), `PlaylistItemView.resume_position_seconds` and `card.playback_progress` / `card.badge` are always hydrated from the **authenticated caller's** `PlaybackState`, never the playlist owner's. Items already marked `"completed"` omit `resume_position_seconds` so queue playback starts them cleanly from `0s`.

#### Multi-User Access Control Matrix

| Operation | Allowed Caller | Private Playlist (`is_public = false`) Not Owned by Caller | Public Playlist (`is_public = true`) Not Owned by Caller |
| :--- | :--- | :--- | :--- |
| **Read / Play / Hydrate** (`GET`, Widget AST) | `owner_user_id == auth_user.id \|\| is_public` | `404 Not Found` (`{"error": "Playlist not found"}`) | `200 OK` (`is_editable: auth_user.role == "admin"`) |
| **Mutate Metadata / Items / Reorder / Delete** (`PATCH`, `POST .../items`, `DELETE .../items/{media_item_id}`, `PUT .../items/order`, `DELETE`) | `owner_user_id == auth_user.id \|\| auth_user.role == "admin"` | `404 Not Found` (existence is never leaked) | `403 Forbidden` (`{"error": "Forbidden"}` unless caller is `admin`) |

### 9.2 Private PIN-Locked Library Isolation

Playlists can contain items from both public libraries and PIN-locked private libraries (`is_private = true`). Kadr enforces fail-closed privacy across every playlist read and write operation:

1. **Zero Metadata Leakage When Locked**: Any item whose parent library is private and not unlocked by a valid `X-Kadr-Unlocked` / `X-Library-Unlock-Token` token is completely excluded from `PlaylistDetail.items`, `cover_poster_urls`, `total_duration_seconds`, and `playlist_items` AST rails. Locked items are counted strictly as an aggregate integer in `PlaylistSummary.locked_item_count` — leaking zero titles, filenames, paths, durations, item IDs, or artwork URLs.
2. **Dense Visible Indexing**: `PlaylistItemView.position` is normalized to `0..items.len() - 1` across the caller's visible slice so locked items never create position gaps or reveal where hidden items sit.
3. **Anti-Probing on `contains_item_id`**: When `GET /api/v1/playlists?contains_item_id=<id>` queries an item residing in a locked private library without its unlock token, `contains_queried_item` evaluates to `false` so callers cannot probe locked-item membership.
4. **`403 LIBRARY_LOCKED` on Locked Add or Remove**: Attempting to add (`POST /api/v1/playlists` with initial `items`, or `POST /api/v1/playlists/{id}/items`) or remove (`DELETE /api/v1/playlists/{id}/items/{media_item_id}`) an item, show, or folder in a locked private library without a valid unlock token immediately fails with `403 Forbidden` and `{"error": "LIBRARY_LOCKED"}`. On 1-step `POST /api/v1/playlists`, item validation runs *before* committing playlist creation so a `LIBRARY_LOCKED` rejection never leaves an orphan empty playlist.
5. **Locked-Preserving Reorder (`PUT /api/v1/playlists/{id}/items/order`)**: Callers submit an exact permutation of their currently **visible** `item_ids` (`0..visible_ids.len() - 1`). Any locked private-library items in the playlist are automatically preserved untouched at positions `visible_ids.len()..(visible_ids.len() + locked_ids.len() - 1)` in their existing relative order, reappearing intact once the library is unlocked.

---

### 9.3 Playlist REST Endpoints

All endpoints below require `Authorization: Bearer <jwt_token>` and accept optional `X-Kadr-Unlocked: <token1>,<token2>` or `X-Library-Unlock-Token: <token>`.

#### 9.3.1 List Visible Playlists — `GET /api/v1/playlists?contains_item_id=<optional i64>`

Lists all playlists owned by `auth_user` plus all public playlists (`is_public == true`) shared by other users on the server, ordered with the caller's own playlists first (`(owner_user_id == auth_user.id) DESC, updated_at DESC`).

- **Query Parameters**:
  - `contains_item_id` *(optional `i64`)*: When provided, populates `contains_queried_item: true | false` on each `PlaylistSummary` (evaluates to `false` if the item does not exist or belongs to a locked private library without an unlock token; omitted from JSON when query param is absent).
- **Response (`200 OK`)**:

```json
{
  "playlists": [
    {
      "id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
      "name": "Sci-Fi Marathon",
      "description": "Weekend director's cuts",
      "owner_user_id": "u-123",
      "owner_username": "alex",
      "is_public": true,
      "is_editable": true,
      "visible_item_count": 4,
      "locked_item_count": 1,
      "total_duration_seconds": 34200,
      "cover_poster_urls": [
        "/api/v1/artwork/10/poster",
        "/api/v1/artwork/14/poster",
        "/api/v1/artwork/22/poster",
        "/api/v1/artwork/31/poster"
      ],
      "created_at": 1762940000,
      "updated_at": 1762941800,
      "contains_queried_item": false
    }
  ]
}
```

---

#### 9.3.2 Create Playlist (with Optional 1-Step Add) — `POST /api/v1/playlists`

Creates a new playlist owned by `auth_user.id`, optionally expanding and seeding initial items in a single atomic workflow.

- **Request Body (`CreatePlaylistRequest`)**:

```json
{
  "name": "Sci-Fi Marathon",
  "description": "Weekend director's cuts",
  "is_public": false,
  "items": {
    "item_ids": [10, 14]
  }
}
```

- **Field Validation**:
  - `name` *(required)*: Trimmed length must be `1..=120` UTF-8 characters (`400 Bad Request` otherwise).
  - `description` *(optional)*: Trimmed length `<= 500` UTF-8 characters (`400 Bad Request` if `> 500`); empty/whitespace string normalizes to `None`.
  - `is_public` *(optional `bool`)*: Defaults to `false`.
  - `items` *(optional `AddPlaylistItemsRequest`)*: Accepts the exact same `item_ids` / `show` / `folder` batch specification as `POST /api/v1/playlists/{id}/items`, validated before playlist creation.
- **Side Effects**: Broadcasts SSE `playlist:updated` (`action: "created"`).
- **Response (`201 Created`)**: Full `PlaylistDetail` (`{ "summary": PlaylistSummary, "items": PlaylistItemView[] }`).

---

#### 9.3.3 Get Playlist Detail & Ordered Queue — `GET /api/v1/playlists/{id}`

Returns the playlist's `PlaylistSummary` and its ordered `PlaylistItemView` list filtered by `UnlockedLibraries` and hydrated with the caller's playback progress.

- **Permissions**: `owner_user_id == auth_user.id || is_public`; otherwise `404 Not Found` (`{"error": "Playlist not found"}`).
- **Response (`200 OK`)**:

```json
{
  "summary": {
    "id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
    "name": "Sci-Fi Marathon",
    "description": "Weekend director's cuts",
    "owner_user_id": "u-123",
    "owner_username": "alex",
    "is_public": true,
    "is_editable": true,
    "visible_item_count": 2,
    "locked_item_count": 0,
    "total_duration_seconds": 16800,
    "cover_poster_urls": [
      "/api/v1/artwork/10/poster",
      "/api/v1/artwork/14/poster"
    ],
    "created_at": 1762940000,
    "updated_at": 1762941800
  },
  "items": [
    {
      "position": 0,
      "added_at": 1762940010,
      "duration_seconds": 8880,
      "resume_position_seconds": 1240,
      "card": {
        "id": 10,
        "title": "Arrival",
        "subtitle": "2016 • 4K",
        "poster_url": "/api/v1/artwork/10/poster",
        "backdrop_url": "/api/v1/artwork/10/backdrop",
        "media_type": "movie",
        "playback_progress": 0.1396,
        "rating": 7.9,
        "release_year": 2016,
        "badge": "RESUME"
      }
    }
  ]
}
```

---

#### 9.3.4 Update Playlist Metadata & Sharing — `PATCH /api/v1/playlists/{id}`

Updates a playlist's `name`, `description`, and/or `is_public` visibility flag.

- **Permissions**: Playlist owner or `admin` (`404` if private to another user, `403` if public and caller is neither owner nor `admin`).
- **Request Body (`UpdatePlaylistRequest`)**:

```json
{
  "name": "Updated Name",
  "description": "Updated description or empty string to clear",
  "is_public": true
}
```

  - All fields are optional.
  - Providing `"description": ""` (empty or whitespace-only string) clears `description` to `NULL` (omitted in subsequent JSON responses).
- **Side Effects**: Updates `updated_at` and broadcasts SSE `playlist:updated` (`action: "updated"`).
- **Response (`200 OK`)**: Updated `PlaylistSummary`.

---

#### 9.3.5 Delete Playlist — `DELETE /api/v1/playlists/{id}`

Deletes the playlist and cascades deletion to all associated `playlist_items` rows.

- **Permissions**: Playlist owner or `admin` (`404` if private to another user, `403` if public and caller is neither owner nor `admin`).
- **Side Effects**: Broadcasts SSE `playlist:updated` (`action: "deleted"`).
- **Response**: `204 No Content`.

---

#### 9.3.6 Add Items (Single, Show/Season, or Folder Batch) — `POST /api/v1/playlists/{id}/items`

Expands, validates, deduplicates, and appends playable media items to the end of a playlist.

- **Permissions**: Playlist owner or `admin` (`404` if invisible, `403` if public non-editable).
- **Request Body (`AddPlaylistItemsRequest`)**:

```json
{
  "item_ids": [10, 14],
  "show": {
    "show_id": 50,
    "season_number": 1
  },
  "folder": {
    "library_id": "lib-uuid",
    "path": "Season 01",
    "recursive": true
  }
}
```

- **Batch Expansion & Validation Rules**:
  - At least one non-empty source (`item_ids`, `show`, or `folder`) must be supplied (`400 Bad Request` `{"error": "No playable media items to add"}` if empty). When multiple sources are provided, they expand deterministically in order: `item_ids`, then `show`, then `folder`.
  1. **`item_ids` (`number[]`)**:
     - Returns `404 Not Found` if any ID does not exist, or `403 Forbidden` `{"error": "LIBRARY_LOCKED"}` if an item's library is private and locked.
     - Rejects non-playable `Season` rows with `400 Bad Request` `{"error": "Non-playable media item type"}`.
     - Automatically expands `Show` items to all episodes of that series in unlocked/public libraries, sorted deterministically by `(season, episode, title, id)`.
     - Appends all playable item types (`Movie`, `Episode`, `Anime`, `Music`, `HomeVideos`, `Audiobook`, `LiveChannel`, `Unknown`) directly.
  2. **`show` (`{ show_id: number, season_number?: number }`)**:
     - Verifies `show_id` exists (`404`), is a `Show` (`400` otherwise), and resides in an unlocked/public library (`403 LIBRARY_LOCKED` if locked).
     - Expands matching episodes (filtered to `season_number` when provided) sorted by `(season, episode, title, id)`, returning `400 Bad Request` `{"error": "No playable episodes found"}` if none match.
  3. **`folder` (`{ library_id: string, path: string, recursive?: boolean }`)**:
     - Verifies `library_id` exists (`404`) and is unlocked (`403 LIBRARY_LOCKED` if private and locked).
     - Sandboxes `path` against configured library roots: rejects `..`, null bytes, or absolute paths (`400 Bad Request` `{"error": "Invalid folder path"}`), returning `404` if the relative directory does not exist.
     - Discovers ingested playable media items in the directory (single-level when `recursive` is `false`/omitted, recursive depth-first walk skipping hidden `.` entries when `recursive` is `true`) ordered lexicographically by relative path, returning `400 Bad Request` `{"error": "No playable media items in folder"}` if none exist.
- **Side Effects**: Appends new items at `max(position) + 1` via `INSERT OR IGNORE`, updates `updated_at`, and broadcasts SSE `playlist:updated` (`action: "items_changed"`).
- **Response (`200 OK`, `AddPlaylistItemsResponse`)**:

```json
{
  "added_count": 8,
  "skipped_duplicate_count": 2,
  "summary": {
    "id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
    "name": "Sci-Fi Marathon",
    "description": "Weekend director's cuts",
    "owner_user_id": "u-123",
    "owner_username": "alex",
    "is_public": true,
    "is_editable": true,
    "visible_item_count": 10,
    "locked_item_count": 0,
    "total_duration_seconds": 54600,
    "cover_poster_urls": [
      "/api/v1/artwork/10/poster",
      "/api/v1/artwork/14/poster",
      "/api/v1/artwork/51/poster",
      "/api/v1/artwork/52/poster"
    ],
    "created_at": 1762940000,
    "updated_at": 1762942100
  }
}
```

---

#### 9.3.7 Remove Item from Playlist — `DELETE /api/v1/playlists/{id}/items/{media_item_id}`

Removes a single item from the playlist and re-compacts remaining item positions to dense `0..N-1` inside a single SQLite transaction.

- **Permissions & Locked-Library Rule**:
  1. Requires playlist edit permission (`owner` or `admin`; `404` if invisible, `403` if public non-editable).
  2. If `media_item_id` belongs to a PIN-locked private library that has not been unlocked via `X-Kadr-Unlocked` / `X-Library-Unlock-Token`, immediately returns `403 Forbidden` `{"error": "LIBRARY_LOCKED"}` (preventing membership probing of locked items).
  3. Returns `404 Not Found` `{"error": "Item not found in playlist"}` if `(id, media_item_id)` is not in the playlist.
- **Side Effects**: Updates `updated_at` and broadcasts SSE `playlist:updated` (`action: "items_changed"`).
- **Response (`200 OK`)**: Updated `PlaylistSummary`.

---

#### 9.3.8 Reorder Visible Playlist Items — `PUT /api/v1/playlists/{id}/items/order`

Reorders the caller's currently visible playlist items while preserving any hidden locked-library items untouched at the tail.

- **Permissions**: Playlist owner or `admin` (`404` if invisible, `403` if public non-editable).
- **Request Body (`ReorderPlaylistItemsRequest`)**:

```json
{
  "item_ids": [14, 10, 31, 22]
}
```

- **Validation**: `item_ids` must contain no duplicates and be an exact set permutation of the playlist's currently visible `media_item_id`s for the caller's `UnlockedLibraries` (`400 Bad Request` `{"error": "item_ids must be an exact permutation of visible playlist items"}` otherwise).
- **Side Effects**: Writes dense `0..visible.len() - 1` positions for `item_ids` (followed by `visible.len()..total - 1` for any locked items in their prior relative order), updates `updated_at`, and broadcasts SSE `playlist:updated` (`action: "items_changed"`).
- **Response (`200 OK`)**: Updated `PlaylistSummary`.

---

### 9.4 Native Client Queue Playback Recipes

Because queue progression is orchestrated client-side while per-item watch states sync via `/api/v1/playback/*`, native Smart TV and mobile apps get full Direct-Play / HLS capability probing and accurate Continue Watching scrobbling on every track in a playlist.

```mermaid
sequenceDiagram
    participant App as Native Client (TV / Mobile)
    participant Server as Kadr Server

    App->>Server: 1. GET /api/v1/playlists/{id} (Bearer + X-Kadr-Unlocked)
    Server-->>App: 200 OK PlaylistDetail (items: PlaylistItemView[])
    App->>App: Hold queue[] & currentIndex = 0 (optional Fisher-Yates shuffle copy)

    loop For active = queue[currentIndex]
        App->>Server: 2. GET /api/v1/items/{active.card.id}
        Server-->>App: 200 OK (inspect technical.container / video_codec / audio_codec)
        App->>Server: 3. POST /api/v1/playback/sessions {"media_item_id": active.card.id}
        Server-->>App: 201 Created {"session_id": "sess-1", ...}
        App->>App: Open Direct Stream or HLS URL; seek if active.resume_position_seconds > 0
        loop Every 10s while playing
            App->>Server: 4. POST /api/v1/playback/sess-1/progress {"position_seconds": pos}
            Server-->>App: 200 OK
        end
        Note over App,Server: On natural EOF (STATE_ENDED / didPlayToEndTime) or Next/Prev skip
        App->>Server: 5. POST /api/v1/playback/sess-1/progress {"position_seconds": finalPos}
        App->>Server: 6. DELETE /api/v1/playback/sessions/sess-1
        Server-->>App: 204 No Content
        App->>App: Advance currentIndex += 1
    end
```

#### Queue Playback Protocol Checklist
1. **Load Queue**: Fetch `GET /api/v1/playlists/{id}` and store `items: PlaylistItemView[]` plus `currentIndex` in memory. For **Shuffle**, run a Fisher-Yates shuffle on a copy of `items` before starting at `currentIndex = 0`.
2. **Per-Track Capability Check**: For `active = items[currentIndex]`, fetch `GET /api/v1/items/{active.card.id}` and inspect `technical` (`container`, `video_codec`, `audio_codec`, `resolution`) to select either Direct Play (`/api/v1/stream/{id}?token=...&unlocked=...`) or Universal HLS fallback (`/api/v1/media/{id}/hls/master.m3u8?video_codec=h264&audio_codec=aac&token=...&unlocked=...`).
3. **Session Start & Resume Seek**: Call `POST /api/v1/playback/sessions` with `{"media_item_id": active.card.id}`. Seek to `active.resume_position_seconds` when present and `> 0`; when omitted (e.g. `watch_state == "completed"` or unwatched), start cleanly at `0s`.
4. **10s Scrobble Heartbeats & Clean Track Transitions**: Send `POST /api/v1/playback/{session_id}/progress` every 10 seconds while playing. On natural EOF, flush one final heartbeat with `position_seconds = active.duration_seconds` so the server marks the item `>= 90%` `completed`, call `DELETE /api/v1/playback/sessions/{session_id}`, increment `currentIndex`, and start the next item. On manual Next/Previous skip, send the current playhead position, close the current session, and start the target track's session.

#### Android / Android TV Queue Controller (Kotlin + AndroidX Media3 `ExoPlayer`)

```kotlin
class KadrPlaylistQueueController(
    private val player: ExoPlayer,
    private val serverBaseUrl: String, // e.g. "http://192.168.1.50:8492"
    private val jwtToken: String,
    private val unlockedHeaderValue: String?, // comma-separated unlock tokens if any
    private val http: OkHttpClient,
    private val scope: CoroutineScope,
) {
    private var queue: List<PlaylistItemView> = emptyList()
    private var currentIndex: Int = 0
    private var activeSessionId: String? = null
    private var heartbeatJob: Job? = null

    init {
        player.addListener(object : Player.Listener {
            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_ENDED) {
                    scope.launch { handleTrackEnded() }
                }
            }
        })
    }

    suspend fun startPlaylist(playlistId: String, shuffle: Boolean = false, startIndex: Int = 0) {
        val detail = fetchPlaylistDetail(playlistId)
        if (detail.items.isEmpty()) return

        queue = if (shuffle) detail.items.toMutableList().apply { shuffle() } else detail.items
        currentIndex = startIndex.coerceIn(0, queue.lastIndex)
        loadAndPlayTrack(currentIndex)
    }

    suspend fun skipNext() {
        if (currentIndex + 1 >= queue.size) return
        val posSec = (player.currentPosition / 1000L).coerceAtLeast(0L)
        closeActiveSession(finalPositionSeconds = posSec)
        currentIndex += 1
        loadAndPlayTrack(currentIndex)
    }

    suspend fun skipPrevious() {
        if (player.currentPosition > 5_000L || currentIndex == 0) {
            player.seekTo(0L)
            return
        }
        val posSec = (player.currentPosition / 1000L).coerceAtLeast(0L)
        closeActiveSession(finalPositionSeconds = posSec)
        currentIndex -= 1
        loadAndPlayTrack(currentIndex)
    }

    private suspend fun handleTrackEnded() {
        val finished = queue.getOrNull(currentIndex) ?: return
        val fullDuration = finished.duration_seconds.takeIf { it > 0 }
            ?: (player.duration.coerceAtLeast(0L) / 1000L)

        // Flush 100% completion position before closing session
        closeActiveSession(finalPositionSeconds = fullDuration)

        if (currentIndex + 1 < queue.size) {
            currentIndex += 1
            loadAndPlayTrack(currentIndex)
        } else {
            player.playWhenReady = false
        }
    }

    private suspend fun loadAndPlayTrack(index: Int) {
        val track = queue[index]
        val itemDetails = fetchItemDetails(track.card.id)
        val streamUri = buildPlaybackUri(track.card.id, itemDetails.technical)

        val sessionId = openPlaybackSession(track.card.id)
        activeSessionId = sessionId

        val mediaItem = MediaItem.fromUri(streamUri)
        player.setMediaItem(mediaItem)
        player.prepare()

        // Completed items omit resume_position_seconds and start cleanly at 0ms
        val startMs = (track.resume_position_seconds ?: 0L).coerceAtLeast(0L) * 1000L
        if (startMs > 0L) player.seekTo(startMs)
        player.playWhenReady = true

        heartbeatJob?.cancel()
        heartbeatJob = scope.launch {
            while (isActive) {
                delay(10_000L)
                if (player.isPlaying) {
                    val sec = (player.currentPosition / 1000L).coerceAtLeast(0L)
                    postProgress(sessionId, sec)
                }
            }
        }
    }

    private fun buildPlaybackUri(itemId: Long, tech: TechnicalInfo?): String {
        val unlockedParam = unlockedHeaderValue?.let { "&unlocked=${Uri.encode(it)}" } ?: ""
        val authQuery = "token=${Uri.encode(jwtToken)}$unlockedParam"
        val needsHls = tech != null && !DeviceCapabilities.supportsDirectPlay(tech)
        return if (needsHls) {
            "$serverBaseUrl/api/v1/media/$itemId/hls/master.m3u8?video_codec=h264&audio_codec=aac&$authQuery"
        } else {
            "$serverBaseUrl/api/v1/stream/$itemId?$authQuery"
        }
    }

    private suspend fun closeActiveSession(finalPositionSeconds: Long) {
        heartbeatJob?.cancel()
        heartbeatJob = null
        val sid = activeSessionId ?: return
        activeSessionId = null
        runCatching { postProgress(sid, finalPositionSeconds) }
        runCatching { deletePlaybackSession(sid) }
    }

    // REST helpers:
    // - fetchPlaylistDetail: GET /api/v1/playlists/{id} (Authorization + X-Kadr-Unlocked)
    // - fetchItemDetails:    GET /api/v1/items/{itemId} (Authorization + X-Kadr-Unlocked)
    // - openPlaybackSession: POST /api/v1/playback/sessions {"media_item_id": itemId}
    // - postProgress:        POST /api/v1/playback/{sid}/progress {"position_seconds": sec}
    // - deletePlaybackSession: DELETE /api/v1/playback/sessions/{sid}
}
```

#### Apple TV & iOS Queue Controller (Swift + `AVPlayer`)

```swift
import AVFoundation
import Foundation

@MainActor
final class KadrQueuePlayerController {
    let player = AVPlayer()

    private let baseURL: URL
    private let jwtToken: String
    private let unlockedTokens: String? // comma-separated unlock tokens if any

    private var queue: [PlaylistItemView] = []
    private var currentIndex: Int = 0
    private var activeSessionId: String?
    private var timeObserver: Any?
    private var endObserver: NSObjectProtocol?

    init(baseURL: URL, jwtToken: String, unlockedTokens: String? = nil) {
        self.baseURL = baseURL
        self.jwtToken = jwtToken
        self.unlockedTokens = unlockedTokens
    }

    func playPlaylist(id: String, shuffle: Bool = false, startIndex: Int = 0) async throws {
        let detail = try await fetchPlaylistDetail(id: id)
        guard !detail.items.isEmpty else { return }

        // Fisher-Yates shuffle via Swift's SystemRandomNumberGenerator on a copy
        self.queue = shuffle ? detail.items.shuffled() : detail.items
        self.currentIndex = min(max(0, startIndex), self.queue.count - 1)
        try await startTrack(at: self.currentIndex)
    }

    func skipNext() async throws {
        guard currentIndex + 1 < queue.count else { return }
        let pos = max(0, Int64(player.currentTime().seconds.rounded(.down)))
        await closeCurrentSession(finalPositionSeconds: pos)
        currentIndex += 1
        try await startTrack(at: currentIndex)
    }

    func skipPrevious() async throws {
        if player.currentTime().seconds > 5.0 || currentIndex == 0 {
            await player.seek(to: .zero)
            return
        }
        let pos = max(0, Int64(player.currentTime().seconds.rounded(.down)))
        await closeCurrentSession(finalPositionSeconds: pos)
        currentIndex -= 1
        try await startTrack(at: currentIndex)
    }

    private func startTrack(at index: Int) async throws {
        teardownObservers()
        let track = queue[index]
        let details = try await fetchItemDetails(itemId: track.card.id)
        let playbackURL = makeStreamURL(itemId: track.card.id, technical: details.technical)

        let session = try await createPlaybackSession(mediaItemId: track.card.id)
        self.activeSessionId = session.session_id

        let item = AVPlayerItem(url: playbackURL)
        player.replaceCurrentItem(with: item)

        if let resume = track.resume_position_seconds, resume > 0 {
            let target = CMTime(seconds: Double(resume), preferredTimescale: 1)
            await player.seek(to: target)
        }

        // 10-second scrobble heartbeat
        let interval = CMTime(seconds: 10, preferredTimescale: 1)
        timeObserver = player.addPeriodicTimeObserver(forInterval: interval, queue: .main) { [weak self] time in
            guard let self, self.player.timeControlStatus == .playing, let sid = self.activeSessionId else { return }
            let sec = max(0, Int64(time.seconds.rounded(.down)))
            Task { try? await self.sendProgress(sessionId: sid, positionSeconds: sec) }
        }

        // Auto-advance on natural EOF
        endObserver = NotificationCenter.default.addObserver(
            forName: .AVPlayerItemDidPlayToEndTime,
            object: item,
            queue: .main
        ) { [weak self] _ in
            guard let self else { return }
            Task { @MainActor in
                let finalSec = max(track.duration_seconds, Int64(item.duration.seconds.rounded(.down)))
                await self.closeCurrentSession(finalPositionSeconds: max(0, finalSec))
                if self.currentIndex + 1 < self.queue.count {
                    self.currentIndex += 1
                    try? await self.startTrack(at: self.currentIndex)
                } else {
                    self.player.pause()
                }
            }
        }

        player.play()
    }

    private func makeStreamURL(itemId: Int64, technical: TechnicalInfo?) -> URL {
        var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false)!
        let useHLS = technical.map { !AVPlayerCapabilities.supportsDirectPlay($0) } ?? false
        var queryItems = [URLQueryItem(name: "token", value: jwtToken)]
        if let unlockedTokens, !unlockedTokens.isEmpty {
            queryItems.append(URLQueryItem(name: "unlocked", value: unlockedTokens))
        }
        if useHLS {
            components.path = "/api/v1/media/\(itemId)/hls/master.m3u8"
            queryItems.insert(URLQueryItem(name: "video_codec", value: "h264"), at: 0)
            queryItems.insert(URLQueryItem(name: "audio_codec", value: "aac"), at: 1)
        } else {
            components.path = "/api/v1/stream/\(itemId)"
        }
        components.queryItems = queryItems
        return components.url!
    }

    private func closeCurrentSession(finalPositionSeconds: Int64) async {
        teardownObservers()
        guard let sid = activeSessionId else { return }
        activeSessionId = nil
        try? await sendProgress(sessionId: sid, positionSeconds: finalPositionSeconds)
        try? await deletePlaybackSession(sessionId: sid)
    }

    private func teardownObservers() {
        if let timeObserver { player.removeTimeObserver(timeObserver) }
        if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
        timeObserver = nil
        endObserver = nil
    }

    // REST helpers attach "Authorization: Bearer \(jwtToken)" and optional "X-Kadr-Unlocked":
    // - fetchPlaylistDetail:   GET /api/v1/playlists/{id}
    // - fetchItemDetails:      GET /api/v1/items/{itemId}
    // - createPlaybackSession: POST /api/v1/playback/sessions {"media_item_id": mediaItemId}
    // - sendProgress:          POST /api/v1/playback/{sessionId}/progress {"position_seconds": positionSeconds}
    // - deletePlaybackSession: DELETE /api/v1/playback/sessions/{sessionId}
}
```

---

## 10. Client Architecture Recipes & Best Practices

### 10.1 Smart TV (10-Foot UI) Guidelines

1. **Remote D-Pad Navigation**:
   - Maintain a dedicated focus manager. Every widget in the AST (`hero_banner`, `carousel`, `grid`) should expose a clear 2D coordinate grid for D-pad directional keys (Up, Down, Left, Right).
   - Use distinct focus halos (e.g. 2px accent outline or scale transform `scale(1.05)`) with subtle easing.
2. **Overscan Margins**:
   - Keep interactive elements within the 90% safe zone (5% padding on top, bottom, left, and right).
3. **Hardware Acceleration**:
   - Ensure media surfaces are hardware-accelerated (`SurfaceView` on Android TV, `AVPlayerLayer` on Apple TV). Avoid rendering transparent UI layers over playing 4K HDR video.

### 10.2 Mobile (iOS & Android) Guidelines

1. **Picture-in-Picture (PiP)**:
   - Wire native PiP delegates. The 10-second heartbeat loop should continue transmitting in the background while PiP is active.
2. **Orientation Switching**:
   - Automatically rotate from portrait to landscape when entering video playback.
3. **Offline Resume Caching**:
   - If network connectivity drops momentarily during video playback, buffer heartbeat requests in a local queue and flush them once the connection is restored.

---

### 10.3 Native Player Setup Snippets

#### Android (Kotlin + AndroidX Media3 / ExoPlayer)

```kotlin
val mediaItem = MediaItem.Builder()
    .setUri("http://$serverIp:8492/api/v1/stream/$itemId?token=$jwtToken")
    .setMimeType(MimeTypes.APPLICATION_MP4)
    .build()

val player = ExoPlayer.Builder(context).build().apply {
    setMediaItem(mediaItem)
    prepare()
    seekTo(resumePositionSeconds * 1000L)
    playWhenReady = true
}
```

#### Apple TV & iOS (Swift + AVFoundation)

```swift
guard let url = URL(string: "http://\(serverIp):8492/api/v1/stream/\(itemId)?token=\(jwtToken)") else { return }

let asset = AVURLAsset(url: url)
let playerItem = AVPlayerItem(asset: asset)
let player = AVPlayer(playerItem: playerItem)

let seekTime = CMTime(seconds: Double(resumePositionSeconds), preferredTimescale: 1)
player.seek(to: seekTime) { _ in
    player.play()
}
```

#### Web (TypeScript + HTML5 Video)

```typescript
const videoElement = document.getElementById('kadr-player') as HTMLVideoElement;
const streamUrl = `/api/v1/stream/${itemId}?token=${encodeURIComponent(token)}`;

videoElement.src = streamUrl;
videoElement.currentTime = resumePositionSeconds;
videoElement.play();

// 10s Scrobble Heartbeat Loop
const intervalId = window.setInterval(async () => {
  if (!videoElement.paused && !videoElement.ended) {
    await fetch(`/api/v1/playback/${sessionId}/progress`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`
      },
      body: JSON.stringify({ position_seconds: Math.floor(videoElement.currentTime) })
    });
  }
}, 10000);
```

#### Samsung Smart TV (Tizen OS + AVPlay / Hls.js)

```javascript
// 1. Inspect item.technical before playback
function getPlaybackUrl(serverUrl, item, token) {
  const tech = item.technical || {};
  const container = (tech.container || '').toLowerCase();
  const audioCodec = (tech.audio_codec || '').toLowerCase();
  const videoCodec = (tech.video_codec || '').toLowerCase();

  // Samsung Tizen does NOT support DTS, DTS-HD, or TrueHD audio.
  // HTML5 video and AVPlay often struggle with raw MKV containers.
  const isAudioUnsupported = ['dts', 'dts-hd', 'truehd'].includes(audioCodec);
  const isContainerUnsupported = ['mkv', 'avi'].includes(container);
  const isVideoUnsupported = !['h264', 'hevc'].includes(videoCodec);

  if (isAudioUnsupported || isContainerUnsupported || isVideoUnsupported) {
    // Universal HLS Fallback:
    // Video is copied (-c:v copy) if compatible (e.g. H.264), audio is transcoded to AAC.
    return `${serverUrl}/api/v1/media/${item.id}/hls/master.m3u8?video_codec=h264&audio_codec=aac&max_height=1080&token=${encodeURIComponent(token)}`;
  }

  // Direct Play compatible
  return `${serverUrl}/api/v1/stream/${item.id}?token=${encodeURIComponent(token)}`;
}

// 2. Play using Tizen AVPlay API
try {
  webapis.avplay.open(getPlaybackUrl(serverUrl, item, token));
  webapis.avplay.setDisplayRect(0, 0, 1920, 1080);
  webapis.avplay.prepareAsync(() => {
    if (resumePositionSeconds > 0) {
      webapis.avplay.seekTo(resumePositionSeconds * 1000);
    }
    webapis.avplay.play();
  });
} catch (e) {
  console.error("AVPlay error:", e);
}
```

---

## 11. Error Handling & HTTP Status Matrix

| HTTP Status | Error Identifier / Response Body | Cause | Client Action |
| :--- | :--- | :--- | :--- |
| `400 Bad Request` | Validation Error | Malformed JSON, empty PIN, or invalid parameter (e.g. non-UUID session ID). | Validate user input before submission. |
| `400 Bad Request` | `{"error": "Playlist name must be between 1 and 120 characters"}` / description length error | Empty playlist name, `name > 120` chars, or `description > 500` chars. | Enforce 1–120 char name and <= 500 char description limits in forms. |
| `400 Bad Request` | `{"error": "No playable media items to add"}` (or `"Non-playable media item type"`, `"No playable episodes found"`, `"No playable media items in folder"`) | Empty batch add request, attempting to add a non-playable `Season` item directly, or adding an empty show/season/folder. | Disable add actions on empty containers or display inline feedback. |
| `400 Bad Request` | `{"error": "Invalid folder path"}` | Playlist folder batch add path contains `..`, null byte, absolute path, or escapes configured library root. | Pass library-relative folder paths returned by `/api/v1/libraries/{id}/folders`. |
| `400 Bad Request` | `{"error": "item_ids must be an exact permutation of visible playlist items"}` | `PUT /api/v1/playlists/{id}/items/order` omits visible items, contains duplicates, or includes unknown/locked IDs. | Refresh `GET /api/v1/playlists/{id}` and submit an exact permutation of visible `card.id`s. |
| `401 Unauthorized` | Invalid PIN / Session Expired | Missing, invalid, or expired JWT token, or incorrect profile PIN. | Prompt user for profile PIN or re-login. |
| `403 Forbidden` | `{"error": "Forbidden"}` | Non-owner, non-admin caller attempting to update, add/remove/reorder items, or delete another user's public playlist. | Hide edit/reorder/delete controls whenever `summary.is_editable == false`. |
| `403 Forbidden` | `{"error": "LIBRARY_LOCKED"}` | Request streams an item or adds/removes a playlist item, show, or folder inside a private library without a valid unlock token. | Present 4-digit Library PIN dialog, call `POST /api/v1/libraries/{id}/unlock`, attach `X-Kadr-Unlocked` / `X-Library-Unlock-Token`, and retry. |
| `404 Not Found` | `{"error": "Playlist not found"}` | Playlist UUID does not exist, or playlist is private (`is_public == false`) and owned by another user. | Remove stale playlist reference and navigate back to Playlists overview. |
| `404 Not Found` | `{"error": "Media item not found"}` / `{"error": "Item not found in playlist"}` (or library/folder/subtitle/session not found) | Referenced `media_item_id`, `library_id`, folder, subtitle, transcode session, or playlist membership row does not exist. | Display a friendly error state; refresh playlist or catalog view. |
| `416 Range Not Satisfiable` | Unsatisfiable Range | Video seek requested a byte offset beyond the file's total size. | Clamp seek offset to `[0, total_size - 1]`. |
| `429 Too Many Requests` | Rate Limited | Exceeded 5 consecutive failed PIN attempts. | Display countdown timer using `retry_after_seconds`. |
| `501 Not Implemented` | `FFMPEG_UNAVAILABLE` | Server host does not have `ffmpeg` installed to transcode HLS fallback. | Install ffmpeg on server or direct-play compatible formats. |
| `503 Service Unavailable` | `TRANSCODE_CAPACITY_EXCEEDED` | Server reached `max_concurrent_transcodes` limit for full video re-encodes. | Notify user to wait or select a direct-play compatible title. |
| `500 Internal Error` | Database / Disk / Transcode Error | Unexpected server or ffmpeg process error. | Log error and offer retry button. |

---

*Document Version: 1.2.0 (Kadr 0.1.3-alpha with Playlists, Queue Playback & Universal HLS Fallback)*  
*Maintained by the Kadr Core Engineering Team.*

