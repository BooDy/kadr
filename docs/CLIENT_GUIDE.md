# Kadr Media Server - Client Developer Guide

Welcome to the **Kadr Client Developer Guide**. This document is the definitive technical reference and implementation manual for building third-party client applications for Kadr, including Smart TV apps (Android TV, Google TV, Apple TV, Fire TV, LG webOS, Samsung Tizen), mobile apps (iOS, iPadOS, Android), desktop applications (macOS, Windows, Linux), and web clients.

---

## Table of Contents

1. [Overview & Protocol Conventions](#1-overview--protocol-conventions)
2. [Network Autodiscovery (`GET /api/v1/discovery`)](#2-network-autodiscovery-get-apiv1discovery)
3. [Authentication, User Profiles & Private Libraries](#3-authentication-user-profiles--private-libraries)
4. [Declarative Screen Layout Engine (AST)](#4-declarative-screen-layout-engine-ast)
5. [Media Details & Direct Folder Navigation](#5-media-details--direct-folder-navigation)
6. [Zero-Transcode Video Streaming & Scrobbling](#6-zero-transcode-video-streaming--scrobbling)
7. [Subtitles & Media Assets](#7-subtitles--media-assets)
8. [Hierarchical Folder Browsing & Real-Time Events (SSE)](#8-hierarchical-folder-browsing--real-time-events-sse)
9. [Client Architecture Recipes & Best Practices](#9-client-architecture-recipes--best-practices)
10. [Error Handling & HTTP Status Matrix](#10-error-handling--http-status-matrix)

---

## 1. Overview & Protocol Conventions

Kadr is a high-performance personal media streaming server written in Rust with the Axum web framework and SQLite (WAL mode). Its architecture differs fundamentally from legacy media servers like Plex or Emby: Kadr adheres strictly to a **zero-transcode direct-play philosophy**.

```mermaid
flowchart LR
    Client["Client App\n(TV / Mobile / Web)"] -- "1. Autodiscover" --> KadrServer["Kadr Media Server\n(Rust Axum Engine)"]
    Client -- "2. Authenticate & Profile PIN" --> KadrServer
    Client -- "3. Fetch Hydrated Screen AST" --> KadrServer
    Client -- "4. HTTP 206 Partial Content (Bytes)" --> Storage[("Disk Storage\nMKV / MP4 / WebM")]
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

### Direct-Play Streaming Philosophy

Kadr **never transcodes video or audio on the server**.
1. **Server Efficiency**: The server acts as an ultra-fast, zero-copy, bounded I/O pipe serving raw container bytes using Tokio asynchronous streams (64 KB chunk size) and standard HTTP 206 range requests.
2. **Client Responsibility**: The client device hardware decodes the video and audio streams natively:
   - **Android / Google TV / Fire TV**: Use **ExoPlayer** (`media3-exoplayer`) with hardware decoders (`MediaCodec`).
   - **Apple TV / iOS / macOS**: Use **AVPlayer** (`AVPlayerItem`).
   - **Desktop**: Embed **libmpv** or **VLCKit** for universal container support (MKV, MP4, AVI, WebM).
   - **Web Browsers**: Native HTML5 `<video>` for browser-compatible formats (H.264/AAC in MP4, VP9/Opus in WebM).

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

### 4.5 The `CardViewModel` Structure

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

### 4.6 Widget Item Pagination

When a user scrolls to the end of a carousel or catalog grid, use the widget data endpoint:

- **Method & Route**: `GET /api/v1/widgets/{widget_id}/data`
- **Query Parameters**:
  - `offset`: Starting index (e.g. `20`)
  - `limit`: Number of items per batch (default: `20`)
  - `screen_id`: Optional screen scope (e.g. `home`, `movies`)
  - `sort`: Optional sort parameter
- **Headers**: `Authorization: Bearer <token>`, optional `X-Library-Unlock-Token: <token>`

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

### 6.2 Playback Session Lifecycle & Scrobble Synchronization

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
```

#### Event Catalog

| Event Name | Payload Structure | Client Action |
| :--- | :--- | :--- |
| `library:updated` | `{"library_id": string, "item_count": number, "timestamp": number}` | Refresh library lists and catalog grids. |
| `session:synced` | `{"session_id": string, "item_id": number, "user_id": string, "position_seconds": number, "timestamp": number}` | Synchronize Continue Watching carousels across other screens without full page reloads. |
| `subtitle:downloaded` | `{"item_id": number, "subtitle_id": number, "language": string, "timestamp": number}` | Refresh in-player subtitle track selection menu. |
| `layout:changed` | `{"screen_id": string, "timestamp": number}` | Invalidate cached screen AST layout and re-render widgets. |
| `system:telemetry` | `{"active_sessions_count": number, "rss_memory_bytes": number, ...}` | Admin dashboard live resource meter update. |

---

## 9. Client Architecture Recipes & Best Practices

### 9.1 Smart TV (10-Foot UI) Guidelines

1. **Remote D-Pad Navigation**:
   - Maintain a dedicated focus manager. Every widget in the AST (`hero_banner`, `carousel`, `grid`) should expose a clear 2D coordinate grid for D-pad directional keys (Up, Down, Left, Right).
   - Use distinct focus halos (e.g. 2px accent outline or scale transform `scale(1.05)`) with subtle easing.
2. **Overscan Margins**:
   - Keep interactive elements within the 90% safe zone (5% padding on top, bottom, left, and right).
3. **Hardware Acceleration**:
   - Ensure media surfaces are hardware-accelerated (`SurfaceView` on Android TV, `AVPlayerLayer` on Apple TV). Avoid rendering transparent UI layers over playing 4K HDR video.

### 9.2 Mobile (iOS & Android) Guidelines

1. **Picture-in-Picture (PiP)**:
   - Wire native PiP delegates. The 10-second heartbeat loop should continue transmitting in the background while PiP is active.
2. **Orientation Switching**:
   - Automatically rotate from portrait to landscape when entering video playback.
3. **Offline Resume Caching**:
   - If network connectivity drops momentarily during video playback, buffer heartbeat requests in a local queue and flush them once the connection is restored.

---

### 9.3 Native Player Setup Snippets

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

---

## 10. Error Handling & HTTP Status Matrix

| HTTP Status | Error Identifier | Cause | Client Action |
| :--- | :--- | :--- | :--- |
| `400 Bad Request` | Validation Error | Malformed JSON, empty PIN, or invalid parameter. | Validate user input before submission. |
| `401 Unauthorized` | Invalid PIN / Session Expired | PIN incorrect or JWT token expired. | Prompt user for profile PIN or re-login. |
| `403 Forbidden` | `LIBRARY_LOCKED` | Request targets an item inside a private library that has not been unlocked. | Present 4-digit Library PIN dialog, submit to `/unlock`, and retry. |
| `404 Not Found` | Not Found | Media item, subtitle, library, or screen does not exist. | Display a friendly error state; return to catalog. |
| `416 Range Not Satisfiable` | Unsatisfiable Range | Video seek requested a byte offset beyond the file's total size. | Clamp seek offset to `[0, total_size - 1]`. |
| `429 Too Many Requests` | Rate Limited | Exceeded 5 consecutive failed PIN attempts. | Display countdown timer using `retry_after_seconds`. |
| `500 Internal Error` | Database / Disk Error | Unexpected server error. | Log error and offer retry button. |

---

*Document Version: 1.0.0 (Kadr 0.1.1-alpha)*  
*Maintained by the Kadr Core Engineering Team.*
