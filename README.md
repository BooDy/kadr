<div align="center">

  <img src="assets/kadr-logo.png" alt="Kadr Logo" width="140" />

  # Kadr (كادر)

  **Minimalist, ultra-high-performance self-hosted media server written in pure Rust**

  [![CI](https://github.com/BooDy/kadr/actions/workflows/ci.yml/badge.svg)](https://github.com/BooDy/kadr/actions/workflows/ci.yml)
  [![Release](https://github.com/BooDy/kadr/actions/workflows/release.yml/badge.svg)](https://github.com/BooDy/kadr/actions/workflows/release.yml)
  [![Version](https://img.shields.io/badge/version-v0.2.0--alpha-blue.svg)](CHANGELOG.md)
  [![GitHub Release](https://img.shields.io/github/v/release/BooDy/kadr?include_prereleases&logo=github&color=blue)](https://github.com/BooDy/kadr/releases)
  [![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg?logo=rust)](https://www.rust-lang.org)
  [![License: GPL-3.0](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
  [![Platform: Linux](https://img.shields.io/badge/Platform-Linux%20musl-lightgrey.svg?logo=linux)](#quick-install-for-users-linux)
  [![Architectures](https://img.shields.io/badge/Arch-x86__64%20%7C%20aarch64-blueviolet.svg)](#quick-install-for-users-linux)
  [![Memory Budget](https://img.shields.io/badge/RSS%20Memory-%E2%89%A4%2030%20MB-emerald.svg)](#key-features)
  [![Zero C Dependencies](https://img.shields.io/badge/Dependencies-Zero%20Native%20C%20(musl)-purple.svg)](#key-features)

</div>

---

**Kadr (كادر)** is a minimalist, ultra-high-performance self-hosted media server written in pure Rust with embedded SQLite (WAL mode) and a responsive, cinema-grade React web client. 

Engineered for resource-constrained single-board computers (Raspberry Pi, low-spec VPS, NAS appliances), Kadr runs in under **30 MB RSS memory** while serving zero-copy HTTP 206 Direct Play video streams, on-demand HLS transcoding & storyboard seek-preview sprite sheets, out-of-process HTTP plugins, ordered playlists, streaming WebVTT subtitle conversions on the fly, declarative layout ASTs, and real-time system telemetry over Server-Sent Events (SSE).

---

## Key Features

- **Blazing Fast & Lightweight**: Operates within a strict $\le 30\text{ MB}$ RSS memory envelope under standard multi-client streaming and layout hydration workloads.
- **Pure-Rust & Portability First**: Zero external C/native runtime dependencies baseline (`reqwest` with `rustls-tls`, pure-Rust crypto via `argon2`, static SQLite engine). Compiles cleanly against `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`.
- **Zero-Copy HTTP 206 Direct Play**: Axum-based streaming pipeline with 64 KiB chunk buffering supporting single and multipart byte-range requests without whole-file memory allocation.
- **Universal On-Demand HLS Transcoding & Smart Stream Copy**: Automatic HTTP Live Streaming (HLS) fallback for Smart TVs (such as Samsung Tizen OS) and web players encountering unsupported audio/video codecs (`DTS`, `TrueHD`, `HEVC 10-bit`, `AV1`) or containers (`MKV`, `AVI`). Features zero-CPU remuxing (`-c:v copy -c:a copy`), smart audio-only transcoding (`-c:v copy -c:a aac`), and full video transcoding (`libx264`, `vaapi`, `nvenc`) with a 15s background reaper sweeping idle sessions (`inactivity_timeout_secs`, default `60`s).
- **Storyboard Seek Preview Thumbnails (Trickplay)**: Lazy on-demand and optional scan-time 10×10 JPEG sprite sheet generation (`ffmpeg` paired with a pure-Rust JPEG SOF dimension parser), serving manifest and cached sprite sheets (`/api/v1/media/:item_id/storyboard`, `/api/v1/media/:item_id/storyboard/:index.jpg`) with `202 Accepted` async generation status and `ETag` / `304 Not Modified` caching, on-demand single-frame extraction (`/api/v1/media/:item_id/frame`), private library lock enforcement, and automatic cache cleanup on re-ingest or deletion.
- **Out-of-Process HTTP Plugin Framework & Live Channels**: Permission-scoped external HTTP plugin processes (`items:publish`, `playback:resolve`) publishing virtual libraries and `live_channel` streams without local files, resolving playback via `direct` (`307 Temporary Redirect`), `proxy` (range-forwarding HTTP relay), or `transcode` (HLS `ffmpeg` pipeline), protected by an SSRF URL guard, per-plugin circuit breaker, 30s background health monitor, live-item scrobble bypass, and live DB-persisted runtime settings.
- **Playlists & Queue Playback**: Manual ordered playlists with per-user ownership and server-wide multi-user sharing (`is_public`), 1-step Create & Add, batch TV Show / Season / sandboxed Folder expansion, fail-closed PIN-locked library isolation (`locked_item_count`), `playlist_items` Declarative Widget AST rails, and multi-track queue playback (auto-advance, Prev/Next, shuffle, and interactive queue popover) in Cinema Player and native clients.
- **Embedded SQLite Storage Engine**: High-concurrency WAL mode (`PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, `PRAGMA busy_timeout = 5000`) with connection pooling and automated schema migrations (`001–009`).
- **Network Autodiscovery & Server Identity**: Zero-conf unauthenticated `/api/v1/discovery` endpoint and persistent server identity configuration for instantaneous client detection across local networks.
- **Declarative Widget AST Layout Engine**: Single-roundtrip screen hydration (`Home`, `Movies`, `Shows`, plus custom screens) powering hero spotlights, horizontal card carousels, and catalog grids with badge computation (`4K`, `NEW`, `RESUME` progress bars).
- **TV Shows & Seasons Grouping**: Automated TV episode filename and directory cue parsing (`S01E02`), parent series aggregation, season tabs navigation, and 1-click episode playback.
- **Hierarchical Folder Browser & Slideshow**: Sandboxed directory browsing with breadcrumbs, direct directory jumping from media details, image discovery (`jpg`, `png`, `webp`, `gif`, `avif`, `bmp`), and full-screen auto-advancing slideshows.
- **Multi-Path & Private PIN-Locked Libraries**: Media libraries spanning multiple local filesystem paths with directory picking, 4-digit PIN protection, database query isolation, and signed HMAC unlock tokens.
- **Online & Local Subtitles with In-Player Search**: Automatic sidecar discovery (`.srt`, `.vtt`, `.ass`, `.sub`), language tag detection, pure-Rust bounded streaming SRT $\to$ WebVTT transcoder, disk caching, and live in-player OpenSubtitles search and on-the-fly download.
- **Video Thumbnail Extraction Engine**: Background `ffmpeg` frame extraction during library scans with SHA-256 disk caching and on-demand fallback routes (`/api/v1/artwork/:item_id/poster`, `/api/v1/libraries/:id/thumbnail`).
- **Real-Time Event Bus (SSE)**: Bounded in-memory event bus (`tokio::sync::broadcast`) broadcasting domain events (`library:updated`, `layout:changed`, `session:synced`, `subtitle:downloaded`, `plugin:error`, `playlist:updated`, `system:telemetry`) over `/api/v1/events` with 15s keep-alive pings.
- **System Telemetry & Health**: Non-blocking collector querying Linux process RSS memory via `/proc/self/statm`, SQLite DB/WAL file sizes, and active playback session counters.
- **Integrated Cinema Web App & Studio**:
  - **Cinema Player**: Full-screen dark video player with multi-track queue playback (auto-advance, Prev/Next track controls, interactive Queue popover, return-to-previous-view navigation), native HTML5 `<track>` WebVTT subtitles, in-player subtitle downloading, auto-resume seeking, 10s playback scrobbling heartbeats, and media keyboard shortcuts (paired with server-side `/storyboard` & `/frame` trickplay APIs for seek-preview clients).
  - **Playlists View**: Top-bar `Playlists` screen with `PlaylistDetailView` (Play All, Shuffle, drag-and-drop / button item reordering, public/private visibility toggle) and 1-step `AddToPlaylistModal` across catalog cards, TV shows/seasons, and folders.
  - **Plugins Admin Panel**: Live administration tab (`Administration > Plugins`) to toggle plugin support and runtime settings without restarts, register plugins by URL, approve requested permissions, copy connection secrets and scoped tokens (`KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`), and monitor plugin health.
  - **Layout Studio**: Visual device simulator (TV 16:9, Tablet 4:3, Mobile 9:16) with widget tree inspection, `playlist_items` rail picker, query filter rules (ratings, genres, exclusions), and custom screen persistence.
  - **Telemetry Dashboard**: Real-time resource gauges and live streaming SSE log feed.
  - **Static File Serving**: Served directly by the Axum server with HTML5 history pushState fallback to `index.html`.

---

## Architecture Overview

```
                            +-----------------------------------------------+
                            |                 Client Tier                   |
                            |   (Web Client / TV Remote / Mobile Device)    |
                            +-------+-------------------------------+-------+
                                    | HTTP / SSE / Discovery        | HTTP 206 / HLS / Storyboards
                                    v                               v
+-----------------------------------------------------------------------------------+
|                                   Kadr Server                                     |
|                                (crates/kadr-server)                               |
|                                                                                   |
|  +-----------------------+   +-----------------------+   +---------------------+  |
|  |    Axum HTTP Router   |   |   Static Web Server   |   |  Direct Play, HLS   |  |
|  | (Auth, Screens, Subs, |   |  (ServeDir / web/dist)|   |  & Storyboard Svc   |  |
|  |  Playlists, Plugins)  |   |                       |   |  (64 KiB Async I/O) |  |
|  +-----------+-----------+   +-----------------------+   +----------+----------+  |
|              |                                                      |             |
|  +-----------v-----------+   +-----------------------+   +----------v----------+  |
|  |  Widget AST Resolver  |   |    Event Bus (SSE)    |   | HTTP Plugin Service |  |
|  |  (Parallel Queries)   |   | (tokio::broadcast)    |   | (Publish & Resolve) |  |
|  +-----------+-----------+   +-----------+-----------+   +----------+----------+  |
|              |                           ^                          |             |
+--------------|---------------------------|--------------------------|-------------+
               |                           |                          |
+--------------v---------------------------|-----------+              |
|        kadr-storage                      |           |              |
| (SQLite WAL Pool, Migrations 001-009)    |           |              |
+--------------+---------------------------+-----------+              |
               |                           |                          |
+--------------v-----------+   +-----------+-----------+              |
|        kadr-ingest       |   |      kadr-core        |              |
| (Inotify Watcher, Probe, |   | (Models, AST, Events, |              |
|  Sidecars, Thumbnails)   |   |  Plugins, Playlists)  |              |
+--------------+-----------+   +-----------------------+              |
               |                                                      |
               v                                                      v
  +-------------------------------------------------------------------------------+
  |                    Local File Storage & External HTTP Plugins                 |
  |     (/movies, /shows, photos, storyboards, thumbnails, .srt, plugin URLs)     |
  +-------------------------------------------------------------------------------+
```

---

## Workspace Structure

The project is organized as a Cargo workspace with four focused crates, a frontend SPA, and a standalone example plugin:

| Directory | Crate / Component | Description |
|:---|:---|:---|
| `crates/kadr-core` | `kadr-core` | Shared domain models, Declarative Widget AST definitions (`playlist_items`, filter rules), plugin & playlist domain types, transcoding decision engine & `ffmpeg` builder, pure-Rust SRT $\to$ WebVTT transcoder, and domain events. |
| `crates/kadr-storage` | `kadr-storage` | Embedded SQLite WAL storage engine, connection pooling, migrations (`001–009`), and repositories (`User`, `Library`, `MediaItem`, `Playback`, `Subtitle`, `Playlist`, `Plugin`, `Settings`). |
| `crates/kadr-ingest` | `kadr-ingest` | Filesystem watcher (`notify`), media scanner, `ffprobe` parser, sidecar subtitle detector, background `ffmpeg` thumbnail extractor, and batch ingestion worker. |
| `crates/kadr-server` | `kadr-server` | Axum HTTP server, JWT/PIN authentication, HTTP 206 Direct Play & HLS fallback streaming, storyboard seek-preview service, out-of-process HTTP plugin framework, ordered playlists & AST widget resolver, subtitle delivery & OpenSubtitles client, SSE event bus, and static web serving. |
| `web/` | `kadr-web` | Modern single-page web app built with React 19, TypeScript, Vite 6, Tailwind CSS, and Lucide React (Browse, Cinema Player, Playlists, Layout Studio, Plugins Admin, and Telemetry Dashboard). |
| `examples/plugin-hello` | `plugin-hello` | Minimal standalone Axum HTTP plugin template publishing virtual sample videos and resolving `direct` playback descriptors. |
| `tests/` & `crates/*/tests/` | Workspace & Crate Tests | Comprehensive unit, integration, and end-to-end test suites covering ingestion, auth, Direct Play & HLS streaming, storyboards, plugins, playlists, AST resolution, subtitles, SSE events, and web serving. |

---

## Quick Install for Users (Linux)

Pre-compiled, zero-dependency static binaries and packages are automatically built and published for **`x86_64` (amd64)** and **`aarch64` (arm64 / Raspberry Pi 4 & 5)** on every release tag (e.g. `v0.2.0-alpha`).

### Option 1: Automated One-Line Installer (Recommended — All Linux Distributions)

On any modern Linux distribution (Fedora, Arch, Alpine, openSUSE, Debian, Ubuntu, RHEL, etc.), run the universal online installer:

```bash
curl -fsSL https://raw.githubusercontent.com/BooDy/kadr/main/scripts/install.sh | sudo bash
```

This automatically detects your CPU architecture, downloads the verified static release archive, creates the `kadr` system user, configures data paths, sets up the systemd service, and starts Kadr immediately.

---

### Option 2: Debian / Ubuntu / Raspberry Pi OS (`.deb`)

Download the appropriate `.deb` package from the [Kadr Releases Page](https://github.com/BooDy/kadr/releases):

```bash
# Install the Debian package (creates 'kadr' user, registers systemd service, installs web app)
sudo dpkg -i kadr_0.2.0_amd64.deb    # For 64-bit PC / Server
# OR
sudo dpkg -i kadr_0.2.0_arm64.deb    # For Raspberry Pi 4/5 / ARM64

# Start Kadr and enable automatic start on system boot
sudo systemctl enable --now kadr

# Verify server status
sudo systemctl status kadr
```

---

### Option 3: Standalone Linux Tarball (`.tar.gz`)

For offline installation on generic Linux distributions:

```bash
# 1. Download and extract the standalone bundle from Releases
tar -xzf kadr-v0.2.0-alpha-x86_64-unknown-linux-musl.tar.gz
cd kadr-v0.2.0-alpha-x86_64-unknown-linux-musl

# 2. Run the automated installer (installs binary, web assets, and systemd service)
sudo ./install.sh

# 3. Start the service
sudo systemctl enable --now kadr
```

---

## Upgrading Kadr

Kadr features zero-downtime, non-destructive upgrades. Upgrading replaces only the server binary (`/usr/local/bin/kadr`) and web client assets (`/usr/share/kadr/web`); your media library records, user accounts, playback history, and custom layouts in `/var/lib/kadr/kadr.db` and configurations in `/etc/kadr/` are **strictly preserved**. Schema migrations execute automatically on startup.

### Method 1: Automated One-Line Upgrade (Recommended)

Simply re-run the universal installer. It detects your existing installation, pauses the active service, deploys the newest version, and restarts Kadr cleanly:

```bash
curl -fsSL https://raw.githubusercontent.com/BooDy/kadr/main/scripts/install.sh | sudo bash
```

### Method 2: Debian / Ubuntu (`.deb`)

Download the updated package and reinstall with `dpkg`:

```bash
sudo dpkg -i kadr_0.2.0_amd64.deb    # or kadr_0.2.0_arm64.deb
sudo systemctl restart kadr
```

### Method 3: Standalone Tarball Upgrade

Extract the new archive and execute the bundled installer (it automatically detects the active service and upgrades in-place):

```bash
tar -xzf kadr-v0.2.0-alpha-x86_64-unknown-linux-musl.tar.gz
cd kadr-v0.2.0-alpha-x86_64-unknown-linux-musl
sudo ./install.sh
```

---

### First-Time Access & Usage

1. **Access Web App**: Open your browser and navigate to `http://localhost:8492` (or `http://<server-ip>:8492`).
2. **Default Login**:
   - Username: `admin`
   - Default PIN: `1234`
3. **Configuration & Media Folders**:
   - Configuration file: `/etc/kadr/kadr.toml`
   - Environment variables: `/etc/kadr/kadr.env`
   - Default data directory: `/var/lib/kadr`
   - To configure media libraries, edit `/etc/kadr/kadr.toml`:
     ```toml
     [[libraries]]
     id = "movies"
     name = "Movies"
     path = "/var/lib/kadr/media/movies"
     media_type = "Movie"

     [[libraries]]
     id = "shows"
     name = "TV Shows"
     path = "/var/lib/kadr/media/shows"
     media_type = "Episode"
     ```
   - Restart after editing: `sudo systemctl restart kadr`
4. **Monitoring & Logs**:
   ```bash
   sudo journalctl -u kadr -f
   ```

---

### Automated Releases & Semantic Versioning

For maintainers, pushing any semantic version tag triggers the automated build and release pipeline:

```bash
git tag -a v0.2.0-alpha -m "Release v0.2.0-alpha"
git push origin v0.2.0-alpha
```

The GitHub Actions release pipeline ([`.github/workflows/release.yml`](.github/workflows/release.yml)) will:
1. Run frontend tests and compile the production React client (`web/dist`).
2. Build static musl binaries for `x86_64` and `aarch64` (Docker cross-compiled).
3. Package standalone `.tar.gz` archives with `install.sh` and systemd units.
4. Package `.deb` installers for `amd64` and `arm64`.
5. Compute SHA256 checksums (`SHA256SUMS.txt`).
6. Publish the GitHub Release with attached assets and release notes.

---

## Building from Source

### Prerequisites

- **Rust**: Version `1.80` or later ([install via rustup](https://rustup.rs))
- **Node.js**: Version `20.x` or later & `npm` (for building the web client)
- **Optional**: `ffmpeg` & `ffprobe` (for detailed video/audio codec extraction, video thumbnail & storyboard sprite-sheet generation, and on-demand HLS transcoding fallback)

---

### 1. Build the Frontend SPA

```bash
cd web
npm install
npm run build
cd ..
```

This compiles the React SPA to optimized static assets in `web/dist`, which the Kadr Rust binary serves automatically.

---

### 2. Configure Kadr

Kadr loads configuration from `kadr.toml` in the current working directory or a custom path via `--config <path>`.

Create or edit `kadr.toml`:

```toml
[server]
name = "Kadr Media Server"
host = "0.0.0.0"
port = 8492
data_dir = "./data"

[storage]
database_path = "./data/kadr.db"
max_readers = 4

[scanner]
debounce_millis = 500
use_ffprobe = true

[transcoding]
enabled = true
ffmpeg_path = "ffmpeg"
max_concurrent_transcodes = 2
inactivity_timeout_secs = 60
temp_dir = "/tmp/kadr/transcodes"
hwaccel = "none" # "none" | "vaapi" | "nvenc"

# Optional storyboard seek-preview thumbnails (trickplay)
[storyboard]
enabled = true
max_concurrent = 1
interval_secs = 10
tile_width = 320
generate_on_scan = false

# Optional HTTP plugin framework defaults (values saved in Administration > Plugins take precedence)
[plugins]
enabled = false
resolve_timeout_secs = 5 # 1-60
allow_private_proxy = false

# Optional media libraries
[[libraries]]
id = "movies-1"
name = "Movies"
path = "/path/to/movies"
media_type = "movie"

[[libraries]]
id = "shows-1"
name = "TV Shows"
path = "/path/to/shows"
media_type = "show"
```

---

### 3. Run Kadr

```bash
# Development mode
cargo run -p kadr-server --bin kadr

# Or with custom config file
cargo run -p kadr-server --bin kadr -- --config /path/to/kadr.toml

# Release mode (maximum performance)
cargo build --release
./target/release/kadr
```

The server will start at `http://localhost:8492` (or configured host/port).

---

### 4. Access the Web Client & Initial Login

1. Open your browser to `http://localhost:8492`.
2. Select the **Admin** user profile.
3. Enter the initial default PIN: `1234`.
4. You are now logged in and can browse media, build playlists, play videos, manage plugins, configure layouts in the Layout Studio, and inspect real-time telemetry!

---

## Development & Testing

### Running Tests

Kadr maintains a 100% passing test suite across all crates, frontend suites, and end-to-end integration tests:

```bash
# Run all Rust workspace unit, integration, and E2E tests (75 integration test targets across 4 crates)
cargo test --workspace

# Run end-to-end user journey test suites specifically
cargo test --test e2e_streaming_auth_test
cargo test --test e2e_hls_transcoding_test
cargo test --test e2e_widget_ast_test
cargo test --test e2e_playlists_test
cargo test --test e2e_subtitles_test
cargo test --test e2e_events_telemetry_test
cargo test --test e2e_web_serving_test
cargo test --test e2e_ingest_test

# Run storyboard trickplay & HTTP plugin framework test suites
cargo test --test storyboard_routes_test --test storyboard_service_test
cargo test --test plugin_admin_test --test plugin_settings_test --test plugin_publish_test \
  --test plugin_playback_test --test plugin_live_playback_test --test plugin_connection_test \
  --test plugin_health_test --test plugin_client_test --test plugin_auth_test --test plugin_url_guard_test

# Run frontend tests (Vitest — 20 test suites)
cd web
npm test -- --run
cd ..

# Verify zero linter warnings across all workspace targets
cargo clippy --workspace --all-targets -- -D warnings
```

### Running Frontend in Hot-Reload Dev Mode

During frontend UI development, you can run the Vite dev server with instant HMR:

```bash
# Terminal 1: Run Rust server
cargo run -p kadr-server --bin kadr

# Terminal 2: Run Vite dev server
cd web
npm run dev
```

The Vite dev server runs at `http://localhost:5173` and automatically proxies `/api` and `/api/v1` to the backend on `http://localhost:8492`.

---

## REST & Streaming API Reference

All protected user/admin endpoints require an `Authorization: Bearer <token>` header (or `?token=<jwt>` query parameter), acquired via PIN authentication. For private PIN-locked libraries, requests must also include the `X-Kadr-Unlocked: <token>` (or `X-Library-Unlock-Token: <token>`) header or `?unlocked=<token>` query parameter. For complete payload schemas, sequence diagrams, and integration recipes, see the [Client Developer Guide](docs/CLIENT_GUIDE.md) and [Plugin Developer Guide](docs/PLUGIN_GUIDE.md).

### Network Autodiscovery & System Configuration
- `GET /api/v1/discovery` — Zero-configuration unauthenticated endpoint returning server identity (`server_id`), `name`, `version`, `protocol_version`, `port`, `setup_completed`, and `status`.
- `GET /api/v1/system/config` — Retrieve runtime server, storage, and scanner configuration (admin).
- `PUT /api/v1/system/config` — Update runtime server `name`, `host`, `port`, `debounce_millis`, or `use_ffprobe` (admin).
- `GET /api/v1/system/fs?path=...` — Server-side filesystem directory picker with mount shortcuts for library creation/editing (admin).

### Authentication & Profiles
- `GET /api/v1/auth/profiles` (or `/api/v1/users/profiles`) — List selectable user profiles (`id`, `username`, `role`, `has_pin`, optional `avatar_color`).
- `POST /api/v1/auth/pin` (or `/api/v1/auth/login`, `/api/v1/auth/profile-pin`) — Authenticate with PIN. Returns JWT token and user info.
- `GET /api/v1/auth/me` — Retrieve authenticated user profile.
- `POST /api/v1/users` — Create a new user profile with PIN and role (admin).

### Libraries & Hierarchical Folder Browser
- `GET /api/v1/libraries` — List configured local and virtual plugin media libraries (`id`, `name`, `path`, `paths`, `media_type`, `is_private`, `created_at`; virtual plugin libraries carry `plugin://<plugin_id>/library/<id>` paths).
- `POST /api/v1/libraries` — Create a media library with optional multi-path roots and 4-digit PIN protection (admin).
- `PATCH /api/v1/libraries/:id` / `DELETE /api/v1/libraries/:id` — Rename a library (`{"name": "..."}`) and sync its screen layout, or delete a library (admin).
- `POST /api/v1/libraries/:id/paths` / `DELETE /api/v1/libraries/:id/paths` — Add or remove filesystem root paths on a library (admin).
- `POST /api/v1/libraries/:id/scan` — Trigger an immediate background scan for a library (admin).
- `POST /api/v1/libraries/:id/unlock` — Unlock a private library with its PIN, returning a signed HMAC unlock token.
- `GET /api/v1/libraries/:id/folders?path=...` — Browse folders, media items, breadcrumbs, and images within sandboxed library paths.
- `GET /api/v1/libraries/:id/image?path=...` — Stream raw image files from sandboxed library folders for gallery and slideshow viewing.
- `GET /api/v1/libraries/:id/thumbnail?path=...` — On-demand cached video thumbnail extraction for folder browser items.

### Video Streaming, HLS Transcoding, Storyboards & Artwork
- `GET /api/v1/stream/:item_id` — Zero-copy HTTP 206 byte-range Direct Play streaming for local files, or plugin playback resolution (`307 Temporary Redirect` for `direct`, range-forwarding HTTP relay for `proxy`, or `409 PLAYBACK_REQUIRES_TRANSCODE` for `transcode`).
- `GET /api/v1/media/:item_id/hls/master.m3u8` — On-demand HLS fallback master playlist with codec negotiation (`video_codec`, `audio_codec`, `max_height`, `audio_track`, `start_time`) for local and plugin media sources.
- `GET /api/v1/media/:item_id/hls/sessions/:session_id/segment_:seq.ts` — Stream 4-second MPEG-TS HLS segments (`video/mp2t`) for an active transcode session.
- `DELETE /api/v1/media/:item_id/hls/sessions/:session_id` — Immediately terminate an active HLS transcode session and clean up temporary segment files.
- `GET /api/v1/media/:item_id/storyboard` — Retrieve storyboard seek-preview manifest (`interval_ms`, `tile_width`, `tile_height`, `columns`, `rows`, `total_tiles`, `sheets`), or `202 Accepted` (`Retry-After: 5`) while background `ffmpeg` sprite generation runs.
- `GET /api/v1/media/:item_id/storyboard/:index.jpg` — Stream cached 10×10 JPEG storyboard sprite sheet (`ETag` / `If-None-Match` `304 Not Modified`, 7-day `Cache-Control`), or `202 Accepted` while generating.
- `GET /api/v1/media/:item_id/frame?t=<seconds>&w=<width>` — Extract and cache a single seek-preview JPEG frame at snapped timestamp `t` and clamped even width `w` (`160..=640`, default `320`).
- `GET /api/v1/artwork/:item_id/poster` — Stream cached poster artwork (`image/jpeg`, `image/png`, `image/webp`, `image/avif`) with on-demand `ffmpeg` video thumbnail fallback.
- `GET /api/v1/artwork/:item_id/backdrop` — Stream cached backdrop artwork.

### Declarative Screens & Widget AST
- `GET /api/v1/screens` / `POST /api/v1/screens` — List available screen layouts (`home`, `movies`, `shows`, plus custom screens) or create a custom screen (admin).
- `GET /api/v1/screens/:screen_id` / `PUT /api/v1/screens/:screen_id` / `DELETE /api/v1/screens/:screen_id` — Retrieve hydrated/unhydrated screen AST, save layout changes (admin), or delete a custom screen (admin).
- `GET /api/v1/widgets/:widget_id/data?offset=0&limit=20` — Fetch paginated items for a specific widget query (including `playlist_items` macro and filter rules).
- `GET /api/v1/items/:item_id` (or `/api/v1/items/:item_id/details`) — Detailed metadata payload for modal inspection (including directory, series/season episodes, and live/plugin fields).

### Playback & Scrobbling
- `POST /api/v1/playback/sessions` — Start a new playback tracking session (`{"media_item_id": 42}`; live items track active session telemetry without persisting watch progress).
- `POST /api/v1/playback/:session_id/progress` — 10-second heartbeat progress update (`{"position_seconds": 120}`).
- `DELETE /api/v1/playback/sessions/:session_id` (or `/api/v1/playback/:session_id`) — Close active playback session.
- `GET /api/v1/playback/states/:item_id` — Fetch user's saved playback position and watch status.
- `GET /api/v1/playback/continue-watching` — List items currently in progress for Continue Watching carousel.

### Playlists & Queue Playback
- `GET /api/v1/playlists?contains_item_id=<optional i64>` — List playlists owned by the user plus shared public playlists (`PlaylistSummary[]`, with optional `contains_queried_item` membership flag).
- `POST /api/v1/playlists` — Create a playlist (`name`, optional `description`, `is_public`, and optional 1-step initial `items`).
- `GET /api/v1/playlists/:id` — Retrieve `PlaylistDetail` (`summary` + ordered visible `PlaylistItemView[]` hydrated with viewer playback progress).
- `PATCH /api/v1/playlists/:id` — Update playlist metadata (`name`, `description`) or server sharing visibility (`is_public`).
- `DELETE /api/v1/playlists/:id` — Delete a playlist and cascade-delete its item entries.
- `POST /api/v1/playlists/:id/items` — Expand, deduplicate, and append playable items (`item_ids`, TV `show`/season, or sandboxed library `folder`).
- `DELETE /api/v1/playlists/:id/items/:media_item_id` — Remove an item from a playlist and re-compact positions (`0..N-1`).
- `PUT /api/v1/playlists/:id/items/order` — Reorder visible playlist items (`{"item_ids": [...]}`) while preserving any PIN-locked private library items.

### HTTP Plugin Framework (Admin & Plugin Publishing)
- `GET /api/v1/admin/plugins/settings` / `PUT /api/v1/admin/plugins/settings` — Retrieve or live-update DB-persisted plugin framework settings (`enabled`, `resolve_timeout_secs`, `allow_private_proxy`) (admin).
- `GET /api/v1/admin/plugins` — List registered plugins with status, permissions, capabilities, and health (admin).
- `POST /api/v1/admin/plugins` — Register an external HTTP plugin by `{"base_url": "http://host:port"}`, fetching its `/manifest.json` and returning `{ plugin, secret }` in `pending` state (admin).
- `POST /api/v1/admin/plugins/:id/approve` — Approve a pending plugin's requested permissions (or subset `{"permissions": [...]}`), enable it, and return `{ plugin, token }` (admin).
- `GET /api/v1/admin/plugins/:id/connection` — Retrieve `{ base_url, secret }` for configuring the plugin process (admin).
- `POST /api/v1/admin/plugins/:id/token` — Issue a fresh scoped plugin-to-server Bearer token `{ token }` for an approved plugin (admin).
- `POST /api/v1/admin/plugins/:id/enable` / `POST /api/v1/admin/plugins/:id/disable` — Toggle an approved plugin without removing its catalog (admin).
- `DELETE /api/v1/admin/plugins/:id?purge=true` — Remove a plugin, optionally purging its virtual libraries and published items (admin).
- `POST /api/v1/plugin/libraries` — Create a plugin-owned virtual library (`{"id"?: "...", "name": "...", "media_type"?: "live_channel"}`; requires plugin Bearer token with `items:publish`).
- `PUT /api/v1/plugin/libraries/:id/items` — Batch-upsert up to 1,000 virtual or live items (`{"items": [...]}`; requires plugin Bearer token with `items:publish`).
- `DELETE /api/v1/plugin/libraries/:id/items/:remote_id` — Delete a published virtual item from a plugin-owned library (requires plugin Bearer token with `items:publish`).

### Subtitle Management
- `GET /api/v1/items/:item_id/subtitles` — List available subtitle tracks for an item.
- `GET /api/v1/subtitles/:subtitle_id/stream.vtt` — Public WebVTT stream (on-the-fly SRT $\to$ WebVTT conversion with disk caching).
- `GET /api/v1/subtitles/:item_id/search?languages=en,ar` — Search online subtitles via OpenSubtitles.
- `POST /api/v1/subtitles/:item_id/download` — Download online subtitle track and register in database.
- `DELETE /api/v1/subtitles/:subtitle_id` — Delete subtitle track and purge cache.

### Real-Time Events & Telemetry
- `GET /api/v1/events` — Public Server-Sent Events (SSE) stream (`library:updated`, `layout:changed`, `session:synced`, `subtitle:downloaded`, `plugin:error`, `playlist:updated`, `system:telemetry`).
- `GET /api/v1/system/telemetry` — Admin-only instantaneous metrics snapshot (`active_sessions_count`, `rss_memory_bytes`, `db_size_bytes`, `wal_size_bytes`, `timestamp`).

---

## Documentation

- [Client Developer Guide](docs/CLIENT_GUIDE.md) — Comprehensive reference for building third-party or native Kadr clients, covering autodiscovery, PIN authentication, declarative layout ASTs, HTTP 206 Direct Play & HLS fallback streaming, storyboard seek-preview trickplay, playlists & queue playback, plugin item playback, scrobbling lifecycle, subtitles, and real-time SSE events.
- [Plugin Developer Guide](docs/PLUGIN_GUIDE.md) — Reference for building and operating out-of-process HTTP plugins, covering Web UI & REST registration/approval, manifest schema, virtual library & live-channel publishing, `direct` / `proxy` / `transcode` playback descriptors, circuit-breaker behavior, and security guards (see [`examples/plugin-hello`](examples/plugin-hello) for a runnable template).
- [Changelog](CHANGELOG.md) — Chronological log of notable additions, improvements, and bug fixes across all versions.
- [Product Requirements Document](kadr_media_server_prd.md) — Architecture requirements, design philosophy, and technical specifications.
- [UI Theme & Design Guidelines](theme.md) — Dark cinema color palette, typography scales, and 10-foot TV UI focus ring specifications.

---

## License

This project is licensed under the GNU General Public License v3.0 — see the [LICENSE](LICENSE) file for details.
