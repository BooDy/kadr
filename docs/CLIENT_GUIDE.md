# Kadr Media Server - Client Developer Guide

Welcome to the **Kadr Client Developer Guide**. This document is the definitive technical reference and implementation manual for building third-party client applications for Kadr, including Smart TV apps (Android TV, Google TV, Apple TV, Fire TV, LG webOS, Samsung Tizen), mobile apps (iOS, iPadOS, Android), desktop applications (macOS, Windows, Linux), and web clients.

---

## Table of Contents

1. [Overview & Protocol Conventions](#1-overview--protocol-conventions)
2. [Network Autodiscovery (`GET /api/v1/discovery`)](#2-network-autodiscovery-get-apiv1discovery)
3. [Authentication, User Profiles & Private Libraries](#3-authentication-user-profiles--private-libraries)
4. [Declarative Screen Layout Engine (AST)](#4-declarative-screen-layout-engine-ast)
5. [Media Details & Direct Folder Navigation](#5-media-details--direct-folder-navigation)
6. [Video Streaming, HLS Transcoding Fallback, Plugin Playback & Scrobbling](#6-video-streaming-hls-transcoding-fallback-plugin-playback--scrobbling)
7. [Subtitles, Storyboard Trickplay & Media Assets](#7-subtitles-storyboard-trickplay--media-assets)
8. [Libraries, Hierarchical Folder Browsing & Real-Time Events (SSE)](#8-libraries-hierarchical-folder-browsing--real-time-events-sse)
9. [Playlists & Client Queue Playback](#9-playlists--client-queue-playback)
10. [Client Architecture Recipes & Best Practices](#10-client-architecture-recipes--best-practices)
11. [Error Handling & HTTP Status Matrix](#11-error-handling--http-status-matrix)

---

## 1. Overview & Protocol Conventions

Kadr is a high-performance personal media streaming server written in Rust with the Axum web framework and SQLite (WAL mode). Its architecture is designed for **instant direct-play streaming**, backed by an **on-demand HLS remuxing/transcoding fallback engine**, **10×10 storyboard sprite trickplay thumbnails**, and **out-of-process HTTP content plugins** (`direct` HTTP `307` redirects, Kadr-proxied streams, live TV channels, and URL-backed HLS transcodes).

```mermaid
flowchart LR
    Client["Client App\n(TV / Mobile / Web)"] -- "1. Autodiscover" --> KadrServer["Kadr Media Server\n(Rust Axum Engine)"]
    Client -- "2. Authenticate & Profile PIN" --> KadrServer
    Client -- "3. Fetch Hydrated Screen AST" --> KadrServer
    Client -- "4a. Direct Play: HTTP 206 / 307" --> Storage[("Disk Storage or\nHTTP Plugin Source")]
    Client -- "4b. Fallback: HLS Master M3U8" --> Transcoder["HlsSessionManager\n(FFmpeg Remux / Transcode)"]
    Client -- "4c. Seek Preview: Storyboard / Frame" --> Storyboard["StoryboardService\n(10x10 JPEG Sprite Sheets)"]
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
| `If-None-Match` | String | Optional conditional `ETag` header (`"<etag>"` or `*`) for `304 Not Modified` validation on storyboard sprite sheets and `/frame` thumbnails |

> [!TIP]
> **Query-String Authentication for Media Players & Image Loaders**: Native media players (`HTMLVideoElement`, `AVPlayer`, `ExoPlayer`, Tizen `AVPlay`) and `<img>` loaders cannot always attach custom HTTP headers to byte-range, HLS segment, or image sub-requests. Kadr accepts query-parameter authentication across streaming (`/api/v1/stream/{id}`), HLS (`/api/v1/media/{id}/hls/master.m3u8`, which automatically propagates auth query params onto rewritten `.ts` segment URLs), and storyboard trickplay (`/api/v1/media/{id}/storyboard`, `/storyboard/{index}.jpg`, `/frame`):
> - `?token=<jwt_token>`
> - `?unlock_token=<unlock_token>` or `?unlocked=<token1>,<token2>`
>
> Using `?token=` on `/api/v1/stream/{id}` also prevents HTTP clients from accidentally leaking `Authorization: Bearer` headers to external hosts when following `307 Temporary Redirect` responses on `direct`-mode plugin streams.

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
  poster_url?: string;            // Relative poster artwork endpoint: "/api/v1/artwork/{id}/poster" (omitted if no local poster)
  backdrop_url?: string;          // Relative backdrop endpoint: "/api/v1/artwork/{id}/backdrop" (omitted if no local backdrop)
  media_type:                     // Serialized in snake_case:
    | "movie"
    | "show"
    | "season"
    | "episode"
    | "anime"
    | "music"
    | "home_videos"
    | "audiobook"
    | "live_channel"
    | "unknown";
  playback_progress?: number;     // Resume position percentage: float in [0.0, 1.0] (omitted when duration_seconds <= 0)
  rating?: number;                // Score (e.g. 8.4)
  release_year?: number;          // Release year (e.g. 2024)
  badge?: string;                 // Visual badge pill: "RESUME" | "NEW" | "4K"
  season?: number;                // Season number (for episodes)
  episode?: number;               // Episode number (for episodes)
}
```

#### Badge & Artwork Rules
- **`RESUME`**: Calculated when the active user has an in-progress playback state (>60 seconds or >2% played, and <90% played). Live channels (`duration_seconds <= 0` or `is_live == true`) never emit `playback_progress` or `"RESUME"`.
- **`NEW`**: Calculated when the media item was ingested or published within the past 14 days.
- **`4K`**: Calculated when technical resolution metadata contains `4K` or `2160`.
- **Optional Artwork URLs**: `poster_url` and `backdrop_url` are omitted when no local artwork file is indexed on disk (for example, plugin-published live channels or remote streams). Client cards should always render a clean fallback placeholder tile (such as a channel monogram or media-type icon) when `poster_url` is absent or returns `404`.

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

### 5.4 Plugin Items & Live Channels (`media_type: "live_channel"`)

When a Kadr server has out-of-process HTTP plugins enabled (see [`docs/PLUGIN_GUIDE.md`](./PLUGIN_GUIDE.md) for plugin author details), plugins can publish virtual VOD items (`"movie"`, `"episode"`, etc.) as well as continuous live TV streams (`media_type: "live_channel"`). Calling `GET /api/v1/items/{item_id}` on a plugin-backed live channel returns:

```json
{
  "card": {
    "id": 512,
    "title": "News 24 HD",
    "subtitle": "1920x1080",
    "media_type": "live_channel",
    "badge": "NEW"
  },
  "overview": "24/7 international live news broadcast.",
  "genres": ["News", "Live"],
  "technical": {
    "duration_seconds": 0,
    "resolution": "1920x1080",
    "video_codec": "h264",
    "audio_codec": "aac",
    "audio_channels": null,
    "container": "mpegts"
  },
  "stream_url": "/api/v1/stream/512",
  "library_id": "iptv-live"
}
```

#### Key Payload Differences for Plugin & Live Items:
- **`duration_seconds` Omission**: Top-level `duration_seconds` is serialized with `skip_serializing_if = "Option::is_none"` and is omitted whenever `technical.duration_seconds <= 0` (while `technical.duration_seconds` is `0`). When `card.media_type === "live_channel"` or `technical.duration_seconds <= 0`, client players should render a **`LIVE`** indicator instead of a scrubbable timeline and skip storyboard/frame preview requests.
- **`folder_path` Omission**: Because plugin items use synthetic `plugin://<plugin_id>/<remote_id>` storage URIs rather than local directories, `folder_path` is omitted (`null` / absent). Clients should hide the **"Browse Folder"** button whenever `!details.folder_path`.
- **`stream_url`**: Always `"/api/v1/stream/{item_id}"`. See [Section 6.4](#64-plugin-backed-playback-resolution-direct-proxy-transcode) for how `/api/v1/stream/{item_id}` and `/api/v1/media/{item_id}/hls/master.m3u8` resolve plugin streams at playback time.

---

## 6. Video Streaming, HLS Transcoding Fallback, Plugin Playback & Scrobbling

Kadr streams local video files directly via HTTP `206 Partial Content`, dynamically resolves plugin-backed streams (`direct` `307` redirects, Kadr-proxied streams, or `transcode` HLS handoffs), and provides an universal `ffmpeg` HLS remux/transcode fallback for incompatible client codecs or containers.

### 6.1 Direct-Play Streaming Endpoint

- **Method & Route**: `GET /api/v1/stream/{item_id}`
- **Authentication**: `Authorization: Bearer <token>` header or `?token=<token>` query parameter (optional for public disk items; identifies `user_id` for plugin `/playback/resolve` calls, defaulting to `"anonymous"` when omitted).
- **Unlock Token (Private Libraries)**: `X-Kadr-Unlocked: <token1>,<token2>` or `X-Library-Unlock-Token: <token>` header, or `?unlocked=<token1>,<token2>` / `?unlock_token=<token>` query parameter (`403 Forbidden` `{"error": "LIBRARY_LOCKED"}` when locked).

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

<binary video stream bytes>
```

#### MIME Types Returned (Local Disk Streams)
| Extension | Content-Type |
| :--- | :--- |
| `.mkv` | `video/x-matroska` |
| `.mp4`, `.m4v` | `video/mp4` |
| `.webm` | `video/webm` |
| `.mov` | `video/quicktime` |
| `.avi` | `video/x-msvideo` |
| `.mp3` | `audio/mpeg` |
| `.flac` | `audio/flac` |
| `.aac` | `audio/aac` |
| `.ogg` | `audio/ogg` |
| `.wav` | `audio/wav` |
| Other | `application/octet-stream` |

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
   - Note that for plugin-published items where `technical` fields are omitted (`null`), clients can attempt `GET /api/v1/stream/{id}` first: if the plugin requires server-side transcoding, Kadr responds with `409 Conflict` (`PLAYBACK_REQUIRES_TRANSCODE`) containing `hls_url` (see [Section 6.4](#64-plugin-backed-playback-resolution-direct-proxy-transcode)).

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
    CheckVideo -- "Yes (or Unknown Plugin Metadata)" --> DirectPlay["Use Direct Stream URL<br/>/api/v1/stream/:id"]
    DirectPlay -- "HTTP 409 PLAYBACK_REQUIRES_TRANSCODE" --> FallbackHLS

    FallbackHLS --> TranscodeMode{"Server Action"}
    TranscodeMode -->|"Video Compatible (e.g. H.264)"| SmartCopy["Smart Stream Copy (-c:v copy)<br/>Transcode only Audio to AAC<br/>0% CPU, Instant Start"]
    TranscodeMode -->|"Video Incompatible (e.g. HEVC 10-bit)"| FullTranscode["Transcode Video (libx264)<br/>Scale resolution if max_height requested"]
```

---

### 6.3 Universal HLS Transcoding & Remuxing Fallback

When direct play is not possible (or when a plugin item returns `409 PLAYBACK_REQUIRES_TRANSCODE`), the client points its player (`AVPlay`, `Hls.js`, `ExoPlayer` `HlsMediaSource`, or `AVPlayer`) to Kadr's HLS master playlist endpoint.

#### 1. Master Playlist Request
- **Method & Route**: `GET /api/v1/media/{item_id}/hls/master.m3u8`
- **Authentication**: `Authorization: Bearer <token>` or `?token=<token>`
- **Unlock Token (Private Libraries)**: `X-Kadr-Unlocked: <token1>,<token2>` / `X-Library-Unlock-Token: <token>` or `?unlocked=<token1>,<token2>` / `?unlock_token=<token>`

##### Query Parameters:
| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `video_codec` | String | `"h264"` | Target video codec required by the client (`"h264"` or `"hevc"`). |
| `audio_codec` | String | `"aac"` | Target audio codec required by the client (`"aac"`, `"ac3"`, `"eac3"`). |
| `max_height` | Integer | Source height | Optional max resolution height: `1080`, `720`, `480`. Automatically scales video down if source exceeds this. |
| `audio_track` | Integer | `0` | Stream index of the audio track to play (also accepts alias `audio_track_index`). |
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
/api/v1/media/101/hls/sessions/e8b6238b-d72b-47e1-85b8-50fa973ff52c/segment_0000.ts?token=eyJhbGci...
#EXTINF:4.000,
/api/v1/media/101/hls/sessions/e8b6238b-d72b-47e1-85b8-50fa973ff52c/segment_0001.ts?token=eyJhbGci...
```

> [!NOTE]
> Any `token=`, `unlocked=`, or `unlock_token=` query parameters passed to `master.m3u8` are automatically preserved and appended onto every rewritten `/segment_{seq}.ts` line so native HLS players do not need custom segment request interceptors.

#### 2. Segment Delivery
- **Method & Route**: `GET /api/v1/media/{item_id}/hls/sessions/{session_id}/segment_{seq}.ts`
- **Response**: `200 OK` (`Content-Type: video/mp2t`, `Cache-Control: public, max-age=3600`)
- **Characteristics**:
  - Enforces private-library `UnlockedLibraries` checks on every segment fetch (`403 LIBRARY_LOCKED` if missing).
  - Validates `session_id` strictly as a UUID and `seq` as a `u32` (eliminating path traversal).
  - Waits up to 15 seconds if `ffmpeg` is still encoding the requested segment (`504 Gateway Timeout` if exceeded) and streams in bounded 64 KB chunks via Tokio `ReaderStream`.
  - Automatically updates the session's `last_accessed_at` timestamp to keep the session alive.

#### 3. HLS Error Statuses, Inactivity Reaper & Explicit Teardown
- **Error Statuses**:
  - `429 Too Many Requests`: `{"error": "Transcoding capacity exceeded"}` when active full video transcodes reach `max_concurrent_transcodes`.
  - `503 Service Unavailable`: `{"error": "Transcoding is disabled"}` when HLS transcoding is disabled in server configuration.
  - `504 Gateway Timeout`: `{"error": "Timed out waiting for <resource>"}` when `ffmpeg` does not produce the initial playlist or a requested segment within 15 seconds.
- **Automatic Inactivity Cleanup**: A background reaper runs every 15 seconds and terminates idle `ffmpeg` sessions (deleting temporary segment files) once `now - last_accessed_at > 60 seconds`.
- **Explicit Session Teardown (Optional)**:
  - **Method & Route**: `DELETE /api/v1/media/{item_id}/hls/sessions/{session_id}`
  - **Response**: `204 No Content` (`404` if session already expired).

---

### 6.4 Plugin-Backed Playback Resolution (`direct`, `proxy`, `transcode`)

Items published by out-of-process HTTP plugins (see [`docs/PLUGIN_GUIDE.md`](./PLUGIN_GUIDE.md) for the plugin `/playback/resolve` contract) are stored with virtual `plugin://<plugin_id>/<remote_id>` paths. When a client requests playback, Kadr first enforces private-library `UnlockedLibraries` checks (`403 LIBRARY_LOCKED` without contacting the plugin) and then resolves a `PlaybackDescriptor` from the owning plugin (caching descriptors in memory when `expires_in_secs > 0`, keyed by `(item_id, user_id, ClientCapabilities)` up to 1024 entries).

```mermaid
sequenceDiagram
    participant Player as Client Player (TV / Mobile / Web)
    participant Kadr as Kadr Server
    participant Plugin as HTTP Plugin (/playback/resolve)
    participant Upstream as External Stream / CDN

    Player->>Kadr: GET /api/v1/stream/{item_id}?token=<jwt>
    Kadr->>Plugin: POST /playback/resolve {remote_id, user_id, client}
    alt mode == "direct"
        Plugin-->>Kadr: 200 OK {mode: "direct", url: "https://cdn.example/live.m3u8"}
        Kadr-->>Player: 307 Temporary Redirect (Location: https://cdn.example/live.m3u8)
        Player->>Upstream: GET https://cdn.example/live.m3u8 (NO Kadr Authorization header!)
    else mode == "proxy"
        Plugin-->>Kadr: 200 OK {mode: "proxy", url: "https://origin.example/video.mp4", headers: {...}}
        Kadr->>Upstream: GET + secret plugin headers (+ Range if seekable)
        Upstream-->>Kadr: 200 OK / 206 Partial Content
        Kadr-->>Player: 200 OK / 206 Partial Content (relayed stream bytes)
    else mode == "transcode"
        Plugin-->>Kadr: 200 OK {mode: "transcode", url: "https://origin.example/live.ts", headers: {...}}
        Kadr-->>Player: 409 Conflict {"error": "PLAYBACK_REQUIRES_TRANSCODE", "hls_url": "/api/v1/media/{id}/hls/master.m3u8"}
        Player->>Kadr: GET /api/v1/media/{id}/hls/master.m3u8?video_codec=h264&audio_codec=aac&token=<jwt>
        Kadr-->>Player: 200 OK (#EXTM3U playlist backed by FFmpeg over upstream URL)
    end
```

#### Playback Mode Behavior Matrix

| Plugin `mode` | `GET /api/v1/stream/{item_id}` Response | `GET /api/v1/media/{item_id}/hls/master.m3u8` Response | Client Responsibility |
| :--- | :--- | :--- | :--- |
| **`"direct"`** | **`307 Temporary Redirect`** with `Location: <descriptor.url>` and empty body. | **`409 Conflict`**: `{"error": "PLAYBACK_DIRECT_ONLY", "url": "<descriptor.url>"}` (never passed to `ffmpeg`). | Follow HTTP `307` redirects (including `http -> https`) **without** forwarding Kadr `Authorization` / `X-Kadr-Unlocked` headers cross-origin. If HLS returned `409 PLAYBACK_DIRECT_ONLY`, open `url` directly. |
| **`"proxy"`** | **`200 OK`**, **`206 Partial Content`**, or **`416 Range Not Satisfiable`**. Kadr injects `descriptor.headers` server-side and relays `Content-Type`, `Content-Length`, `Content-Range`, and `Accept-Ranges`. | **`200 OK`** (`application/vnd.apple.mpegurl`). Kadr passes the SSRF-validated URL and `descriptor.headers` to `ffmpeg`. | Treat identically to a local Kadr stream. Note: `Range` is forwarded for seekable items (`!live` and `seekable != false`) and ignored on live/non-seekable streams. |
| **`"transcode"`** | **`409 Conflict`**: `{"error": "PLAYBACK_REQUIRES_TRANSCODE", "hls_url": "/api/v1/media/{item_id}/hls/master.m3u8"}`. | **`200 OK`** (`application/vnd.apple.mpegurl`). Forwards `ClientCapabilities { codecs: [video_codec, audio_codec], containers: [], max_height }` to the plugin and spawns `ffmpeg`. | On `409 PLAYBACK_REQUIRES_TRANSCODE`, switch the player to `hls_url` (appending `video_codec`, `audio_codec`, optional `max_height`, and `?token=` / `&unlocked=`). |

> [!CAUTION]
> **Critical Client Rules for `direct` (`307 Temporary Redirect`) Streams**:
> 1. **Enable Cross-Protocol Redirects (`http` -> `https`)**: Self-hosted Kadr instances commonly run on plain LAN HTTP (`http://192.168.1.50:8492`) while plugins redirect to `https://` CDNs. Android's `HttpURLConnection` / Media3 `DefaultHttpDataSource` **blocks** `http -> https` redirects by default unless `.setAllowCrossProtocolRedirects(true)` is explicitly set!
> 2. **Never Leak Kadr Credentials Cross-Origin**: Do **not** attach `Authorization: Bearer <jwt>` or `X-Kadr-Unlocked` / `X-Library-Unlock-Token` on an HTTP client configured to blindly copy request headers onto cross-origin `307` redirect targets. Always authenticate `/api/v1/stream/{item_id}` via query parameters (`?token=<jwt>&unlocked=<unlock_token>`) — or strip `Authorization` and `X-Kadr-*` / `X-Library-*` headers whenever `redirectUri.host != kadrUri.host`.

#### Plugin Upstream Failure & Circuit-Breaker Status Codes

When a plugin cannot resolve or proxy an item, both `/api/v1/stream/{item_id}` and `/api/v1/media/{item_id}/hls/master.m3u8` return structured JSON `{"error": "<code>", "message": "<reason>"}`:

| HTTP Status | `error` Code | Cause |
| :--- | :--- | :--- |
| `503 Service Unavailable` | `"PLUGIN_UNAVAILABLE"` | Plugin subsystem is disabled, owning plugin is uninstalled / `disabled` / `pending` / `unhealthy`, lacks `playback:resolve` permission, failed connection (retried once on connection errors only — never retried on HTTP `5xx` responses or timeouts) or returned `5xx`, upstream proxy connection failed, or the per-plugin **circuit breaker** is open (opens for **30 seconds** after **3 consecutive failures**, allowing a single half-open probe after cooldown). |
| `504 Gateway Timeout` | `"PLUGIN_TIMEOUT"` | Plugin `/playback/resolve` call or upstream `proxy` connection exceeded `resolve_timeout_secs` (default **5 seconds**, configurable `1..=60s`). |
| `502 Bad Gateway` | `"PLUGIN_BAD_RESPONSE"` | Plugin returned `4xx` or malformed JSON, invalid URL scheme / CRLF header injection, private/loopback/link-local proxy address rejected by Kadr's SSRF guard (`allow_private_proxy = false` by default), or `proxy` upstream returned a status outside `{200, 206, 416}` (upstream `3xx` redirects are not followed in `proxy` mode). |

---

### 6.5 Playback Session Lifecycle & Scrobble Synchronization

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
        Kadr-.->SSE: Broadcast SSE event: "session:synced" (VOD items only)
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

##### Automatic Server-Side Scrobble Evaluation Rules (VOD Items):
The server evaluates watch state according to strict threshold rules:
1. **`Unwatched`**: Position < 60 seconds and played < 2% of total duration.
2. **`InProgress`**: Position > 60 seconds or played > 2% of total duration. The item appears in `Continue Watching` shelves with an accurate resume progress bar.
3. **`Completed`**: Position reaches **90% or higher** of total duration. Play count increments by 1, and resume position resets to 0.

##### Live Item Session & Scrobbling Behavior (`is_live` / `LiveChannel`):
Clients can run the **exact same** session lifecycle for live plugin streams without special-casing REST calls:
- When a plugin publishes an item with `"live": true` (`item.is_live() == true`), `POST /api/v1/playback/sessions` registers an active in-memory session (`is_live: true`) in `SessionRegistry` and returns `201 Created` with `"resume_position_seconds": 0` (never loading or resuming prior watch state).
- Subsequent `POST /api/v1/playback/{session_id}/progress` heartbeats return `200 OK` and update active in-memory session telemetry (`current_position_seconds` and `last_heartbeat_at` for admin active-stream monitoring), **while completely skipping persistent watch-progress scrobbling**:
  - `user_playback_states` is **not** written in SQLite, so live channels never pollute `Continue Watching` rails even if a plugin supplies a finite program window `duration_seconds`.
  - `session:synced` SSE events are **not** broadcast for `is_live` sessions.
- Calling `DELETE /api/v1/playback/sessions/{session_id}` (or `DELETE /api/v1/playback/{session_id}`) removes the active live session and returns `204 No Content`.

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

## 7. Subtitles, Storyboard Trickplay & Media Assets

Kadr provides first-class subtitle delivery with on-the-fly WebVTT conversion, disk caching, in-player online subtitle search via OpenSubtitles, high-resolution artwork delivery, and 10×10 storyboard trickplay sprite sheets with single-frame seek fallback.

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

### 7.5 Storyboard Seek Preview Thumbnails (Trickplay) & Frame Fallback

For local on-disk video items, Kadr generates **10×10 JPEG storyboard sprite sheets** (100 tiles per sheet) in a bounded background `ffmpeg` worker pool (`[storyboard]` in `kadr.toml`), paired with an on-demand single-frame JPEG fallback endpoint (`/frame`) so players can display instant visual seek previews even while sprite sheets are still generating.

#### Authentication & Private Library Unlock Support (All 3 Endpoints)

Unlike `GET /api/v1/stream/{item_id}` (where `AuthUser` is optional on public items), **all three storyboard and frame endpoints strictly require user authentication**:
- **User Token (Required)**: `Authorization: Bearer <jwt_token>` header **or** `?token=<jwt_token>` query parameter. Missing or invalid tokens return `401 Unauthorized` (`{"error": "Missing authorization token"}` or `{"error": "Invalid or expired token"}`).
- **Private Library Unlock (When `is_private == true`)**: `X-Kadr-Unlocked: <token1>,<token2>` or `X-Library-Unlock-Token: <token>` header **or** `?unlocked=<token1>,<token2>` / `?unlock_token=<token>` query parameter. Locked libraries return `403 Forbidden` (`{"error": "LIBRARY_LOCKED"}`).
- **Unsupported / Non-Video Items (`404 Not Found`)**: Returns `404 Not Found` (`{"error": "Media item has no video stream"}`) for `Music`, `Audiobook`, `Show`, `Season`, and `LiveChannel` items, all plugin-backed virtual items (`plugin://<plugin_id>/<remote_id>`), items with `duration_seconds <= 0`, audio-only files (`.mp3`, `.flac`, `.aac`, `.ogg`, `.wav`, `.m4a`, `.opus` with no video codec), and files marked `.novideo` after `ffmpeg` probes zero video frames.
- **Subsystem Availability (`503 Service Unavailable`)**: Returns `503 Service Unavailable` (`{"error": "Storyboard generation unavailable"}`) when `storyboard.enabled = false` or `ffmpeg` is not installed on the server host.

---

#### 1. Storyboard Manifest — `GET /api/v1/media/{item_id}/storyboard`

Returns the item's sprite grid geometry and ordered sheet URLs if already cached on disk, or enqueues background generation and returns `202 Accepted`.

- **Method & Route**: `GET /api/v1/media/{item_id}/storyboard`
- **Response (`200 OK` — `application/json`, `StoryboardManifest`)**:

```json
{
  "interval_ms": 10000,
  "tile_width": 320,
  "tile_height": 134,
  "columns": 10,
  "rows": 10,
  "total_tiles": 680,
  "sheets": [
    {
      "index": 0,
      "url": "/api/v1/media/101/storyboard/0.jpg",
      "first_tile": 0,
      "tile_count": 100
    },
    {
      "index": 1,
      "url": "/api/v1/media/101/storyboard/1.jpg",
      "first_tile": 100,
      "tile_count": 100
    },
    {
      "index": 6,
      "url": "/api/v1/media/101/storyboard/6.jpg",
      "first_tile": 600,
      "tile_count": 80
    }
  ]
}
```

##### Manifest Field & Interval Semantics:
| Field | Type | Description |
| :--- | :--- | :--- |
| `interval_ms` | `u64` | Milliseconds between consecutive tiles. Defaults to `5000` (5s) when `duration_seconds < 600` (10 min) and `10000` (10s, `storyboard.interval_secs * 1000`) for longer titles; automatically widened (`ceil(duration_seconds / 1000) * 1000`) on multi-hour titles so `total_tiles <= 1000` (at most 10 sprite sheets). |
| `tile_width` | `u32` | Pixel width of a single tile (`storyboard.tile_width`, default `320`, clamped to `160..=640` and rounded down to an even integer). |
| `tile_height` | `u32` | Aspect-ratio-preserving even pixel height (`>= 2`) parsed directly from the generated JPEG SOF0/SOF1/SOF2 header (`(sheet_height / 10) & !1`). |
| `columns` | `u32` | Grid columns per sheet (`10`). |
| `rows` | `u32` | Grid rows per sheet (`10`). |
| `total_tiles` | `u32` | Total extracted tiles across all sheets (`1..=1000`). |
| `sheets[].index` | `u32` | 0-based sprite sheet index (`0..sheets.len() - 1`). |
| `sheets[].url` | `String` | Clean server-relative path `"/api/v1/media/{item_id}/storyboard/{index}.jpg"` (**never** embeds `?token=` or `?unlocked=` — clients must attach headers or append query auth when fetching sheets). |
| `sheets[].first_tile` | `u32` | Global 0-based tile index of the top-left tile `(col=0, row=0)` in this sheet (`index * 100`). |
| `sheets[].tile_count` | `u32` | Active tiles populated in this sheet (`100` on full sheets; `1..=100` on the final sheet). Note: the JPEG canvas dimensions are always `(columns * tile_width) × (rows * tile_height)` even on a partial final sheet. |

- **Response When Generating (`202 Accepted`)**:
  ```http
  HTTP/1.1 202 Accepted
  Content-Type: application/json
  Retry-After: 5

  {"status":"generating"}
  ```

---

#### 2. Storyboard Sprite Sheet — `GET /api/v1/media/{item_id}/storyboard/{index}.jpg`

Streams a single `10×10` JPEG sprite sheet with immutable content-hash `ETag` validation and 7-day HTTP caching.

- **Method & Route**: `GET /api/v1/media/{item_id}/storyboard/{index}.jpg` (`{index}` must be an unsigned `u32` integer with `.jpg` suffix, e.g. `0.jpg`; any other filename returns `404` `{"error": "Storyboard sheet not found"}`).
- **Optional Request Header**: `If-None-Match: "<source_hash>-<index>"` (or `*`).
- **Response (`200 OK`)**:
  ```http
  HTTP/1.1 200 OK
  Content-Type: image/jpeg
  Cache-Control: public, max-age=604800
  ETag: "9f86d081884c7d65-0"
  Content-Length: 184320

  <JPEG sprite sheet bytes>
  ```
- **Conditional Cache Hit (`304 Not Modified`)**: Returned with empty body, `ETag`, and `Cache-Control: public, max-age=604800` when `If-None-Match` matches.
- **In-Flight Generation (`202 Accepted`)**: Returns `Retry-After: 5` and `{"status": "generating"}` if `index < planned_sheet_count` while the background worker is still building the storyboard.
- **Out-of-Range Index (`404 Not Found`)**: Returns `{"error": "Storyboard sheet index out of range"}` when `index >= sheets.len()`.

---

#### 3. On-Demand Single-Frame Fallback — `GET /api/v1/media/{item_id}/frame`

Synchronously extracts (with a 2-permit concurrency semaphore and on-disk frame cache) a single JPEG frame near timestamp `t`. Used as an instant seek-preview fallback while `/storyboard` returns `202 Accepted`.

- **Method & Route**: `GET /api/v1/media/{item_id}/frame?t=<seconds>&w=<width>`
- **Query Parameters**:
  | Parameter | Type | Default | Validation & Clamping |
  | :--- | :--- | :--- | :--- |
  | `t` | `f64` | `0.0` | Target seek timestamp in seconds. Snapped to integer seconds via `round(t)` and clamped to `0..=duration_seconds`. Returns **`400 Bad Request`** `{"error": "Invalid frame timestamp"}` if `t < 0.0`, `NaN`/`±Infinity`, or `duration_seconds <= 0`. |
  | `w` | `u32` | `320` | Target frame width in pixels. Clamped to `160..=640` and rounded down to an even integer (`w.clamp(160, 640) & !1`). |
- **Response (`200 OK` / `304 Not Modified`)**:
  - `Content-Type: image/jpeg`
  - `Cache-Control: public, max-age=604800`
  - `ETag: "frame-<frame_hash>"` (supports `If-None-Match` -> `304 Not Modified`)
  - `Content-Length: <bytes>`

---

#### 4. Sprite Tile Coordinate Math (From Timestamp `t` in Seconds)

Given a `StoryboardManifest` and a scrubbed target timestamp `t` (in seconds), compute the exact sprite sheet URL and crop rectangle `(x, y, width, height)` in pixels:

```typescript
interface TileCrop {
  sheetUrl: string;
  sheetWidth: number;
  sheetHeight: number;
  col: number;
  row: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export function resolveStoryboardTile(
  manifest: StoryboardManifest,
  tSeconds: number,
  authQuerySuffix = "", // e.g. "?token=...&unlocked=..." when loading via <img> / CSS background-image
): TileCrop | null {
  if (manifest.total_tiles <= 0 || manifest.sheets.length === 0 || manifest.interval_ms <= 0) {
    return null;
  }

  const tilesPerSheet = manifest.columns * manifest.rows; // 100
  const clampedMs = Math.max(0, tSeconds) * 1000;

  // 1. Global 0-based tile index clamped to [0, total_tiles - 1]
  const tileIndex = Math.min(
    manifest.total_tiles - 1,
    Math.floor(clampedMs / manifest.interval_ms),
  );

  // 2. Locate owning sheet and local 0-based index within that sheet
  const sheetIndex = Math.floor(tileIndex / tilesPerSheet);
  const sheet =
    manifest.sheets[sheetIndex] ??
    manifest.sheets.find(
      (s) => tileIndex >= s.first_tile && tileIndex < s.first_tile + s.tile_count,
    );
  if (!sheet) return null;

  const localIndex = tileIndex - sheet.first_tile;

  // 3. Grid row/column & pixel crop coordinates
  const col = localIndex % manifest.columns;
  const row = Math.floor(localIndex / manifest.columns);
  const x = col * manifest.tile_width;
  const y = row * manifest.tile_height;

  return {
    sheetUrl: `${sheet.url}${authQuerySuffix}`,
    sheetWidth: manifest.columns * manifest.tile_width,
    sheetHeight: manifest.rows * manifest.tile_height,
    col,
    row,
    x,
    y,
    width: manifest.tile_width,
    height: manifest.tile_height,
  };
}
```

---

#### 5. Fallback to `/frame?t=...` & Native TV D-Pad / Mobile Scrubber Guidance

```mermaid
stateDiagram-v2
    [*] --> CheckEligible: Player opens item
    CheckEligible --> Disabled: live_channel, duration <= 0, or audio-only
    CheckEligible --> FetchManifest: GET /api/v1/media/{id}/storyboard
    FetchManifest --> Ready: 200 OK (StoryboardManifest)
    FetchManifest --> Generating: 202 Accepted (Retry-After: 5)
    FetchManifest --> Disabled: 404 No video / 503 Unavailable
    Generating --> FetchManifest: Poll after 5s (cap ~12 retries)
    Generating --> FrameFallback: User scrubs while generating -> debounced GET /frame?t=...&w=320
    Ready --> SpriteCrop: 0ms local crop (x, y, tile_width, tile_height)
```

1. **While `202 Accepted` (`{"status": "generating"}`)**:
   - Respect the `Retry-After: 5` header when polling `GET /api/v1/media/{item_id}/storyboard` in the background (cap polling to ~12 attempts or cancel immediately when the player unmounts).
   - If the user scrubs before the manifest arrives, debounce scrub changes by **150–250 ms**, snap `t` to `2s`–`5s` increments (so nearby drags hit cached URLs), cancel superseded requests (`AbortController` / `Call.cancel()`), and display `GET /api/v1/media/{item_id}/frame?t=<snappedSec>&w=320&token=<jwt>`. Once `/storyboard` returns `200 OK`, switch seamlessly to sprite-sheet cropping.
2. **Smart TV 10-Foot D-Pad Trickplay (`Android TV`, `Apple TV`, `Tizen`, `webOS`)**:
   - **Virtual Scrub Playhead**: When the user presses `DPAD_LEFT` or `DPAD_RIGHT`, enter a *scrub preview state* advancing a virtual `previewSeconds` cursor (`±10s` per tap, accelerating to `±30s` / `±60s` on key-hold) **without** calling `player.seekTo(...)` on every key repeat. Decoding keyframes on every D-pad repeat stalls hardware decoders; instead, keep video paused or playing smoothly while updating the floating `resolveStoryboardTile(...)` thumbnail card at 60 fps, and commit a single `player.seekTo(previewSeconds * 1000)` when the user presses `DPAD_CENTER` / `Enter` (or after an 800 ms idle settle timer).
   - **Adjacent Sheet Prefetching**: Decode and hold `sheets[sheetIndex]` in an LRU bitmap cache (3–4 sheets, ~2–4 MB RAM) and prefetch `sheets[sheetIndex + 1]` as `localIndex` crosses tile `80` so crossing a 100-tile (~16.6 min) boundary never flickers.
   - **Android / Compose / `BitmapRegionDecoder` or `Canvas.drawBitmap`**: Crop `Rect(crop.x, crop.y, crop.x + crop.width, crop.y + crop.height)` from the cached `Bitmap` directly onto your seekbar preview `ImageView` / `Canvas`.
3. **Mobile Touch Scrubber (`iOS` & `Android`)**:
   - Anchor a floating `tile_width × tile_height` rounded preview window (`160×67` pt on phones, `240×100` pt on tablets) centered horizontally above the user's thumb X-coordinate (clamped to screen safe-area margins) while dragging the seek slider, committing the player seek on `onTouchUp` / `.onEnded`.

---

## 8. Libraries, Hierarchical Folder Browsing & Real-Time Events (SSE)

### 8.1 Listing Libraries & Virtual Plugin Libraries (`GET /api/v1/libraries`)

Clients can query all registered libraries on the server (both local disk libraries and plugin-published virtual libraries):

- **Method & Route**: `GET /api/v1/libraries`
- **Authentication**: Public / `Authorization: Bearer <token>`
- **Response (`200 OK` — `Library[]`)**:

```json
[
  {
    "id": "lib-movies-001",
    "name": "Movies",
    "path": "/media/movies",
    "paths": ["/media/movies", "/mnt/nas2/movies"],
    "media_type": "movie",
    "is_private": false,
    "created_at": 1728300000
  },
  {
    "id": "iptv-live",
    "name": "Live Channels",
    "path": "plugin://iptv/library/iptv-live",
    "paths": ["plugin://iptv/library/iptv-live"],
    "media_type": "live_channel",
    "is_private": false,
    "created_at": 1762941000
  }
]
```

#### Identifying Virtual Plugin Libraries & Client Behavior:
- **Exact Wire Fields**: Every `Library` object serializes `id`, `name`, `path`, `paths`, `media_type`, `is_private`, and `created_at` (`pin_hash` is never serialized, and SQLite's internal `plugin_id` ownership column is **not** exposed as a JSON field on `Library`).
- **Detecting Virtual Plugin Libraries**: Plugins register virtual libraries with synthetic `plugin://<plugin_id>/library/<library_id>` URIs in `path` and `paths` (see [`docs/PLUGIN_GUIDE.md`](./PLUGIN_GUIDE.md)). Clients can detect a virtual library via `library.path.startsWith("plugin://")`.
- **Catalog vs. Folder Browsing**:
  - Virtual plugin libraries work identically to local libraries across screen AST layouts (`GET /api/v1/screens/{screen_id}`), widget pagination (`GET /api/v1/widgets/{widget_id}/data`), item details (`GET /api/v1/items/{item_id}`), and Playlists (`/api/v1/playlists`).
  - Filesystem-only endpoints (`GET /api/v1/libraries/{id}/folders`, `/thumbnail`, and `/image`) require a real directory on disk and return **`404 Not Found`** (`{"error": "Library path not found on disk"}`) for `plugin://` virtual libraries. Clients should hide the **"Folder View"** tab whenever `library.path.startsWith("plugin://")`.

---

### 8.2 Browsing Library Folders

For local disk libraries organized in customized directory hierarchies (e.g. `Movies/Sci-Fi/Franchise/`), Kadr provides full directory navigation.

- **Method & Route**: `GET /api/v1/libraries/{id}/folders`
- **Query Parameters**:
  - `path`: Subdirectory path relative to the library root (e.g. `path=Sci-Fi/Dune`). Omit for root.
- **Headers**: `Authorization: Bearer <token>`, optional `X-Kadr-Unlocked: <token1>,<token2>` / `X-Library-Unlock-Token: <token>`
- **Response Code**: `200 OK` (`404 Not Found` `{"error": "Library path not found on disk"}` if called on a `plugin://` virtual library)

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

### 8.3 Real-Time Server-Sent Events (SSE)

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

event: plugin:error
data: {"type":"plugin:error","payload":{"plugin_id":"iptv","message":"Plugin unavailable: health check returned 503 Service Unavailable","timestamp":1762942200}}
```

#### Event Catalog

| Event Name | Payload Structure | Client Action |
| :--- | :--- | :--- |
| `library:updated` | `{"library_id": string, "item_count": number, "timestamp": number}` | Refresh library lists and catalog grids (emitted for both disk scans and plugin item upserts/deletes). |
| `session:synced` | `{"session_id": string, "item_id": number, "user_id": string, "position_seconds": number, "timestamp": number}` | Synchronize Continue Watching carousels across other screens without full page reloads. |
| `subtitle:downloaded` | `{"item_id": number, "subtitle_id": number, "language": string, "timestamp": number}` | Refresh in-player subtitle track selection menu. |
| `layout:changed` | `{"screen_id": string, "timestamp": number}` | Invalidate cached screen AST layout and re-render widgets. |
| `playlist:updated` | `{"playlist_id": string, "user_id": string, "action": string, "timestamp": number}` | Refresh playlist lists, active playlist detail views, or `playlist_items` AST rails matching `playlist_id`. |
| `plugin:error` | `{"plugin_id": string, "message": string, "timestamp": number}` | Surface an admin toast or mark items/libraries from `plugin_id` as temporarily degraded when an enabled plugin transitions to `unhealthy`. |
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

#### `plugin:error` Payload

```json
{
  "type": "plugin:error",
  "payload": {
    "plugin_id": "iptv",
    "message": "Plugin unavailable: health check returned 503 Service Unavailable",
    "timestamp": 1762942200
  }
}
```

- Emitted when Kadr's 30-second background plugin health monitor detects that a previously `enabled` plugin failed its `/health` check and transitions its status to `unhealthy`. While `unhealthy`, playback resolution requests for that plugin's items immediately fail closed with `503 Service Unavailable` (`"PLUGIN_UNAVAILABLE"`) until a subsequent `/health` check succeeds and restores `enabled` status.

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

> [!TIP]
> **Playlists Containing Plugin Items or Live Channels**: When a playlist includes plugin-backed items or `media_type === "live_channel"` entries, configure your player datasource with cross-protocol `307` redirect following (`setAllowCrossProtocolRedirects(true)` on ExoPlayer) without forwarding Kadr `Authorization` headers cross-origin, fall back to `hls_url` if `/api/v1/stream/{id}` returns `409 PLAYBACK_REQUIRES_TRANSCODE` (see [Section 6.4](#64-plugin-backed-playback-resolution-direct-proxy-transcode)), and skip initial resume seeks on continuous live channels (`active.duration_seconds == 0`).

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
// Allow HTTP -> HTTPS 307 redirects for plugin "direct" streams without leaking
// Authorization headers cross-origin (authenticate Kadr via ?token= query param).
val httpDataSourceFactory = DefaultHttpDataSource.Factory()
    .setAllowCrossProtocolRedirects(true)
val mediaSourceFactory = DefaultMediaSourceFactory(context)
    .setDataSourceFactory(httpDataSourceFactory)

val authQuery = "token=${Uri.encode(jwtToken)}" +
    (unlockedToken?.let { "&unlocked=${Uri.encode(it)}" } ?: "")
val directUri = "http://$serverIp:8492/api/v1/stream/$itemId?$authQuery"
val hlsFallbackUri =
    "http://$serverIp:8492/api/v1/media/$itemId/hls/master.m3u8?video_codec=h264&audio_codec=aac&$authQuery"

val player = ExoPlayer.Builder(context)
    .setMediaSourceFactory(mediaSourceFactory)
    .build()

player.addListener(object : Player.Listener {
    override fun onPlayerError(error: PlaybackException) {
        val cause = error.cause
        // If a plugin item requires transcoding, /api/v1/stream/{id} returns HTTP 409
        if (cause is HttpDataSource.InvalidResponseCodeException && cause.responseCode == 409) {
            player.setMediaItem(MediaItem.fromUri(hlsFallbackUri))
            player.prepare()
            player.playWhenReady = true
        }
    }
})

player.setMediaItem(MediaItem.fromUri(directUri))
player.prepare()
if (!isLiveChannel && resumePositionSeconds > 0) {
    player.seekTo(resumePositionSeconds * 1000L)
}
player.playWhenReady = true
```

#### Apple TV & iOS (Swift + AVFoundation)

```swift
// Authenticate via ?token= query parameter so URLSession automatically follows
// 307 Temporary Redirects on plugin "direct" streams without leaking Kadr headers.
var query = "token=\(jwtToken.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "")"
if let unlockedToken {
    query += "&unlocked=\(unlockedToken.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "")"
}
guard let url = URL(string: "http://\(serverIp):8492/api/v1/stream/\(itemId)?\(query)") else { return }

let asset = AVURLAsset(url: url)
let playerItem = AVPlayerItem(asset: asset)
let player = AVPlayer(playerItem: playerItem)

if !isLiveChannel && resumePositionSeconds > 0 {
    let seekTime = CMTime(seconds: Double(resumePositionSeconds), preferredTimescale: 1)
    player.seek(to: seekTime) { _ in
        player.play()
    }
} else {
    player.play()
}
```

#### Web (TypeScript + HTML5 Video / Hls.js)

```typescript
const videoElement = document.getElementById('kadr-player') as HTMLVideoElement;
const authQuery = `token=${encodeURIComponent(token)}${unlocked ? `&unlocked=${encodeURIComponent(unlocked)}` : ''}`;
const streamUrl = `/api/v1/stream/${itemId}?${authQuery}`;
const hlsUrl = `/api/v1/media/${itemId}/hls/master.m3u8?video_codec=h264&audio_codec=aac&${authQuery}`;

// Optional HEAD/redirect-manual or error fallback: if /stream/{id} returns 409 PLAYBACK_REQUIRES_TRANSCODE,
// attach Hls.js (or native Safari HLS) against hlsUrl.
videoElement.src = streamUrl;
if (!isLiveChannel && resumePositionSeconds > 0) {
  videoElement.currentTime = resumePositionSeconds;
}
videoElement.play();

// 10s Scrobble Heartbeat Loop (safe for both VOD and live sessions; server skips DB persistence when is_live)
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
function getPlaybackUrl(serverUrl, item, token, forceHls = false) {
  const auth = `token=${encodeURIComponent(token)}`;
  if (forceHls) {
    return `${serverUrl}/api/v1/media/${item.id}/hls/master.m3u8?video_codec=h264&audio_codec=aac&max_height=1080&${auth}`;
  }

  const tech = item.technical || {};
  const container = (tech.container || '').toLowerCase();
  const audioCodec = (tech.audio_codec || '').toLowerCase();
  const videoCodec = (tech.video_codec || '').toLowerCase();

  // Samsung Tizen does NOT support DTS, DTS-HD, or TrueHD audio.
  // HTML5 video and AVPlay often struggle with raw MKV containers.
  const isAudioUnsupported = ['dts', 'dts-hd', 'truehd'].includes(audioCodec);
  const isContainerUnsupported = ['mkv', 'avi'].includes(container);
  const isVideoUnsupported = Boolean(videoCodec) && !['h264', 'hevc'].includes(videoCodec);

  if (isAudioUnsupported || isContainerUnsupported || isVideoUnsupported) {
    // Universal HLS Fallback (Note: if a plugin item is "direct"-only, master.m3u8 returns
    // 409 {"error": "PLAYBACK_DIRECT_ONLY", "url": "..."} which can be opened directly).
    return `${serverUrl}/api/v1/media/${item.id}/hls/master.m3u8?video_codec=h264&audio_codec=aac&max_height=1080&${auth}`;
  }

  // Direct Play / Plugin 307 Redirect / Plugin Proxy compatible
  return `${serverUrl}/api/v1/stream/${item.id}?${auth}`;
}

// 2. Play using Tizen AVPlay API
try {
  webapis.avplay.open(getPlaybackUrl(serverUrl, item, token));
  webapis.avplay.setDisplayRect(0, 0, 1920, 1080);
  webapis.avplay.prepareAsync(() => {
    if (item.card?.media_type !== 'live_channel' && resumePositionSeconds > 0) {
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
| `202 Accepted` | `{"status": "generating"}` (+ `Retry-After: 5`) | `GET /api/v1/media/{id}/storyboard` or `/storyboard/{index}.jpg` enqueued or awaiting background `ffmpeg` sprite extraction. | Retry `/storyboard` after `Retry-After` seconds (`5s`); optionally render debounced `/api/v1/media/{id}/frame?t=...` previews meanwhile. |
| `304 Not Modified` | Empty body (+ `ETag` & `Cache-Control: public, max-age=604800`) | Client `If-None-Match` header matched cached storyboard sheet (`"<hash>-<index>"`) or frame (`"frame-<hash>"`). | Reuse cached JPEG sprite sheet or frame bitmap. |
| `307 Temporary Redirect` | Empty body + `Location: <external_url>` | `GET /api/v1/stream/{id}` resolved a `"direct"`-mode plugin item. | Follow HTTP redirect (including cross-protocol `http -> https`) **without** forwarding Kadr `Authorization` or `X-Kadr-Unlocked` headers cross-origin. |
| `400 Bad Request` | Validation Error | Malformed JSON, empty PIN, or invalid parameter (e.g. non-UUID session ID). | Validate user input before submission. |
| `400 Bad Request` | `{"error": "Invalid frame timestamp"}` | `GET /api/v1/media/{id}/frame` called with `t < 0`, non-finite `t`, or on an item with `duration_seconds <= 0`. | Clamp `t` to `[0, duration_seconds]` and skip `/frame` when `duration_seconds <= 0`. |
| `400 Bad Request` | `{"error": "Playlist name must be between 1 and 120 characters"}` / description length error | Empty playlist name, `name > 120` chars, or `description > 500` chars. | Enforce 1–120 char name and <= 500 char description limits in forms. |
| `400 Bad Request` | `{"error": "No playable media items to add"}` (or `"Non-playable media item type"`, `"No playable episodes found"`, `"No playable media items in folder"`) | Empty batch add request, attempting to add a non-playable `Season` item directly, or adding an empty show/season/folder. | Disable add actions on empty containers or display inline feedback. |
| `400 Bad Request` | `{"error": "Invalid folder path"}` | Playlist folder batch add path contains `..`, null byte, absolute path, or escapes configured library root. | Pass library-relative folder paths returned by `/api/v1/libraries/{id}/folders`. |
| `400 Bad Request` | `{"error": "item_ids must be an exact permutation of visible playlist items"}` | `PUT /api/v1/playlists/{id}/items/order` omits visible items, contains duplicates, or includes unknown/locked IDs. | Refresh `GET /api/v1/playlists/{id}` and submit an exact permutation of visible `card.id`s. |
| `401 Unauthorized` | `{"error": "Missing authorization token"}` / `{"error": "Invalid or expired token"}` / Invalid PIN | Missing, invalid, or expired JWT (`Authorization: Bearer` / `?token=`), or incorrect profile PIN. | Prompt user for profile PIN or re-login. |
| `403 Forbidden` | `{"error": "Forbidden"}` | Non-owner, non-admin caller attempting to update, add/remove/reorder items, or delete another user's public playlist. | Hide edit/reorder/delete controls whenever `summary.is_editable == false`. |
| `403 Forbidden` | `{"error": "LIBRARY_LOCKED"}` | Request accesses `/stream/{id}`, `/hls/*`, `/storyboard*`, `/frame`, `/items/{id}`, `/libraries/{id}/folders`, or adds/removes a locked private-library item in a playlist without a valid unlock token. | Present 4-digit Library PIN dialog, call `POST /api/v1/libraries/{id}/unlock`, attach `X-Kadr-Unlocked` / `X-Library-Unlock-Token` (or `?unlocked=`), and retry. |
| `404 Not Found` | `{"error": "Playlist not found"}` | Playlist UUID does not exist, or playlist is private (`is_public == false`) and owned by another user. | Remove stale playlist reference and navigate back to Playlists overview. |
| `404 Not Found` | `{"error": "Media item not found"}` / `{"error": "Item not found in playlist"}` / `{"error": "Media file missing from disk"}` | Referenced `media_item_id`, `library_id`, folder, subtitle, HLS session, or underlying media file does not exist. | Display a friendly error state; refresh playlist or catalog view. |
| `404 Not Found` | `{"error": "Media item has no video stream"}` / `{"error": "Storyboard sheet index out of range"}` / `{"error": "Storyboard sheet not found"}` / `{"error": "Frame not found"}` | Storyboard or `/frame` requested for a plugin item, `LiveChannel`, audio/container item, `duration_seconds <= 0`, `.novideo` file, or out-of-range sheet index. | Disable visual scrub thumbnails for the active session and show timestamp pill only. |
| `404 Not Found` | `{"error": "Library path not found on disk"}` | `GET /api/v1/libraries/{id}/folders` (or `/thumbnail`, `/image`) called on a virtual plugin library (`path` starting with `"plugin://"`). | Hide Folder Explorer navigation whenever `library.path.startsWith("plugin://")`. |
| `409 Conflict` | `{"error": "PLAYBACK_REQUIRES_TRANSCODE", "hls_url": "/api/v1/media/{id}/hls/master.m3u8"}` | `GET /api/v1/stream/{id}` resolved a `"transcode"`-mode plugin item that cannot be raw-streamed. | Switch player to `hls_url` (appending `video_codec`, `audio_codec`, optional `max_height`, and `?token=` / `&unlocked=`). |
| `409 Conflict` | `{"error": "PLAYBACK_DIRECT_ONLY", "url": "<external_url>"}` | `GET /api/v1/media/{id}/hls/master.m3u8` called on a `"direct"`-mode plugin item (never passed to server `ffmpeg`). | Open `url` directly in the client media player (or play `/api/v1/stream/{id}`). |
| `416 Range Not Satisfiable` | Unsatisfiable Range | Video seek requested a byte offset beyond the file's or upstream proxy resource's total size. | Clamp seek offset to `[0, total_size - 1]`. |
| `429 Too Many Requests` | Rate Limited / `{"error": "Transcoding capacity exceeded"}` | Exceeded 5 consecutive failed PIN attempts (`retry_after_seconds`), or `HlsSessionManager` reached `max_concurrent_transcodes`. | Display PIN lockout countdown or notify user that server transcode slots are full. |
| `502 Bad Gateway` | `{"error": "PLUGIN_BAD_RESPONSE", "message": "..."}` | Plugin returned `4xx` / malformed descriptor, proxy target blocked by SSRF private-IP guard, or upstream `proxy` origin returned status outside `{200, 206, 416}`. | Show upstream source error toast and offer retry or alternate title. |
| `503 Service Unavailable` | `{"error": "PLUGIN_UNAVAILABLE", "message": "..."}` | Plugins disabled, owning plugin missing / `disabled` / `pending` / `unhealthy`, missing `playback:resolve` permission, plugin `5xx` / connect error, or per-plugin circuit breaker open (30s cooldown after 3 consecutive failures). | Surface plugin unavailable banner; retry after cooldown or inspect `plugin:error` SSE status. |
| `503 Service Unavailable` | `{"error": "Storyboard generation unavailable"}` / `{"error": "Transcoding is disabled"}` | Storyboard subsystem disabled / `ffmpeg` unavailable on server, or HLS transcoding disabled in config. | Fall back to timestamp-only scrubbing or direct-play compatible formats. |
| `504 Gateway Timeout` | `{"error": "PLUGIN_TIMEOUT", "message": "Plugin timed out"}` / `{"error": "Timed out waiting for ..."}` | Plugin `/playback/resolve` or proxy upstream exceeded `resolve_timeout_secs` (default 5s), or `ffmpeg` timed out generating HLS playlist/segment. | Show timeout message with retry action. |
| `500 Internal Server Error` | Database / Disk / Transcode Error | Unexpected SQLite, disk I/O, storyboard read, or `ffmpeg` process failure. | Log error and offer retry button. |

---

*Document Version: 1.3.0 (Kadr 0.1.3-alpha with Storyboard Trickplay, Plugin Playback, Playlists & Universal HLS Fallback)*  
*Maintained by the Kadr Core Engineering Team.*
