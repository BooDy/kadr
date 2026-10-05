<div align="center">

  <img src="assets/kadr-logo.png" alt="Kadr Logo" width="140" />

  # Kadr (كادر)

  **Minimalist, ultra-high-performance self-hosted media server written in pure Rust**

  [![CI](https://github.com/BooDy/kadr/actions/workflows/ci.yml/badge.svg)](https://github.com/BooDy/kadr/actions/workflows/ci.yml)
  [![Release](https://github.com/BooDy/kadr/actions/workflows/release.yml/badge.svg)](https://github.com/BooDy/kadr/actions/workflows/release.yml)
  [![GitHub Release](https://img.shields.io/github/v/release/BooDy/kadr?include_prereleases&logo=github&color=blue)](https://github.com/BooDy/kadr/releases)
  [![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg?logo=rust)](https://www.rust-lang.org)
  [![License: GPL-3.0](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
  [![Platform: Linux](https://img.shields.io/badge/Platform-Linux%20musl-lightgrey.svg?logo=linux)](#quick-install-for-users-linux)
  [![Architectures](https://img.shields.io/badge/Arch-x86__64%20%7C%20aarch64-blueviolet.svg)](#quick-install-for-users-linux)
  [![Memory Budget](https://img.shields.io/badge/RSS%20Memory-%E2%89%A4%2030%20MB-emerald.svg)](#performance--constraints)
  [![Zero C Dependencies](https://img.shields.io/badge/Dependencies-Zero%20Native%20C%20(musl)-purple.svg)](#portability)

</div>

---

**Kadr (كادر)** is a minimalist, ultra-high-performance self-hosted media server written in pure Rust with embedded SQLite (WAL mode) and a responsive, cinema-grade React web client. 

Engineered for resource-constrained single-board computers (Raspberry Pi, low-spec VPS, NAS appliances), Kadr runs in under **30 MB RSS memory** while serving zero-copy HTTP 206 Direct Play video streams, streaming WebVTT subtitle conversions on the fly, hydrating declarative layout ASTs, and broadcasting real-time system telemetry over Server-Sent Events (SSE).

---

## Key Features

- **Blazing Fast & Lightweight**: Operates within a strict $\le 30\text{ MB}$ RSS memory envelope under standard multi-client streaming and layout hydration workloads.
- **Pure-Rust & Portability First**: Zero external C/native runtime dependencies baseline (`reqwest` with `rustls-tls`, pure-Rust crypto via `argon2`, static SQLite engine). Compiles cleanly against `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`.
- **Zero-Copy HTTP 206 Direct Play**: Axum-based streaming pipeline with 64 KiB chunk buffering supporting single and multipart byte-range requests without whole-file memory allocation.
- **Embedded SQLite Storage Engine**: High-concurrency WAL mode (`PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, `PRAGMA busy_timeout = 5000`) with connection pooling and automated schema migrations (001–004).
- **Declarative Widget AST Layout Engine**: Single-roundtrip screen hydration (`Home`, `Movies`, `Shows`) powering hero spotlights, horizontal card carousels, and catalog grids with badge computation (`4K`, `NEW`, `RESUME` progress bars).
- **Online & Local Subtitles**: Automatic sidecar discovery (`.srt`, `.vtt`, `.ass`, `.sub`), language tag detection, pure-Rust bounded streaming SRT $\to$ WebVTT transcoder, on-the-fly disk caching, and OpenSubtitles.com REST API client.
- **Real-Time Event Bus (SSE)**: Bounded in-memory event bus (`tokio::sync::broadcast`) broadcasting domain events (`library:updated`, `session:synced`, `subtitle:downloaded`, `system:telemetry`) over `/api/v1/events` with 15s keep-alive pings.
- **System Telemetry & Health**: Non-blocking collector querying Linux process RSS memory via `/proc/self/statm`, SQLite DB/WAL file sizes, and active playback session counters.
- **Integrated Cinema Web App & Studio**:
  - **Cinema Player**: Full-screen dark video player with native HTML5 `<track>` WebVTT subtitles, auto-resume seeking, 10s playback scrobbling heartbeats, and media keyboard shortcuts.
  - **Layout Studio**: Visual device simulator (TV 16:9, Tablet 4:3, Mobile 9:16) with widget tree inspection and live preview toggles.
  - **Telemetry Dashboard**: Real-time resource gauges and live streaming SSE log feed.
  - **Static File Serving**: Served directly by the Axum server with HTML5 history pushState fallback to `index.html`.

---

## Architecture Overview

```
                            +-----------------------------------------------+
                            |                 Client Tier                   |
                            |   (Web Client / TV Remote / Mobile Device)    |
                            +-------+-------------------------------+-------+
                                    | HTTP / SSE                    | HTTP 206 Streaming
                                    v                               v
+-----------------------------------------------------------------------------------+
|                                   Kadr Server                                     |
|                                (crates/kadr-server)                               |
|                                                                                   |
|  +-----------------------+   +-----------------------+   +---------------------+  |
|  |    Axum HTTP Router   |   |   Static Web Server   |   |  HTTP Range Stream  |  |
|  | (Auth, Screens, Subs) |   |  (ServeDir / web/dist)|   | (64 KiB Async I/O)  |  |
|  +-----------+-----------+   +-----------------------+   +----------+----------+  |
|              |                                                      |             |
|  +-----------v-----------+   +-----------------------+              |             |
|  |  Widget AST Resolver  |   |    Event Bus (SSE)    |              |             |
|  |  (Parallel Queries)   |   | (tokio::broadcast)    |              |             |
|  +-----------+-----------+   +-----------+-----------+              |             |
|              |                           ^                          |             |
+--------------|---------------------------|--------------------------|-------------+
               |                           |                          |
+--------------v---------------------------|-----------+              |
|        kadr-storage                      |           |              |
| (SQLite WAL Pool, Migrations 001-004)    |           |              |
+--------------+---------------------------+-----------+              |
               |                           |                          |
+--------------v-----------+   +-----------+-----------+              |
|        kadr-ingest       |   |      kadr-core        |              |
| (Inotify Watcher, Probe, |   | (Models, AST, Events, |              |
|  Sidecars, Worker)       |   |  SRT->VTT Transcoder) |              |
+--------------+-----------+   +-----------------------+              |
               |                                                      |
               v                                                      v
  +-------------------------------------------------------------------------------+
  |                          Local File Storage                                   |
  |                (/movies, /shows, .srt sidecars, artwork)                      |
  +-------------------------------------------------------------------------------+
```

---

## Workspace Structure

The project is organized as a Cargo workspace with four focused crates and a frontend SPA:

| Directory | Crate / Component | Description |
|:---|:---|:---|
| `crates/kadr-core` | `kadr-core` | Shared domain models, Declarative Widget AST definitions, pure-Rust SRT $\to$ WebVTT transcoder, and domain events. |
| `crates/kadr-storage` | `kadr-storage` | Embedded SQLite WAL storage engine, connection pooling, migrations (001–004), and repositories (`User`, `MediaItem`, `Playback`, `Subtitle`). |
| `crates/kadr-ingest` | `kadr-ingest` | Filesystem watcher (`notify`), media scanner, ffprobe parser, sidecar subtitle detector, and batch ingestion worker. |
| `crates/kadr-server` | `kadr-server` | Axum HTTP server, JWT/PIN authentication, HTTP 206 streaming, AST widget resolver, subtitle delivery service, OpenSubtitles client, SSE event bus, and static web serving. |
| `web/` | `kadr-web` | Modern single-page web app built with React 19, TypeScript, Vite 6, Tailwind CSS, and Lucide React. |
| `tests/` | Workspace Tests | Comprehensive end-to-end user journey tests covering ingestion, auth, streaming, AST resolution, subtitles, events, and web serving. |

---

## Quick Install for Users (Linux)

Pre-compiled, zero-dependency static binaries and packages are automatically built and published for **`x86_64` (amd64)** and **`aarch64` (arm64 / Raspberry Pi 4 & 5)** on every release tag (e.g. `v0.1.0-alpha.1`).

### Option 1: Debian / Ubuntu / Raspberry Pi OS (`.deb`)

Download the appropriate `.deb` package from the [Kadr Releases Page](https://github.com/BooDy/kadr/releases):

```bash
# Install the Debian package (creates 'kadr' user, registers systemd service, installs web app)
sudo dpkg -i kadr_0.1.0_amd64.deb    # For 64-bit PC / Server
# OR
sudo dpkg -i kadr_0.1.0_arm64.deb    # For Raspberry Pi 4/5 / ARM64

# Start Kadr and enable automatic start on system boot
sudo systemctl enable --now kadr

# Verify server status
sudo systemctl status kadr
```

---

### Option 2: Standalone Linux Tarball (`.tar.gz`)

For generic Linux distributions (Arch, Alpine, Fedora, openSUSE, etc.):

```bash
# 1. Download and extract the standalone bundle from Releases
tar -xzf kadr-v0.1.0-alpha.1-x86_64-unknown-linux-musl.tar.gz
cd kadr-v0.1.0-alpha.1-x86_64-unknown-linux-musl

# 2. Run the automated installer (installs binary, web assets, and systemd service)
sudo ./install.sh

# 3. Start the service
sudo systemctl enable --now kadr
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
git tag -a v0.1.0-alpha.1 -m "Release v0.1.0-alpha.1"
git push origin v0.1.0-alpha.1
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
- **Optional**: `ffprobe` (for detailed video/audio codec extraction; falls back to metadata inference if omitted)

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
host = "0.0.0.0"
port = 8492
data_dir = "./data"

[storage]
database_path = "./data/kadr.db"
max_readers = 4

[scanner]
debounce_millis = 500
use_ffprobe = true

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
media_type = "series"
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
4. You are now logged in and can browse media, play videos, configure layouts in the Layout Studio, and inspect real-time telemetry!

---

## Development & Testing

### Running Tests

Kadr maintains a 100% passing test suite across all crates, frontend suites, and end-to-end integration tests:

```bash
# Run all Rust workspace tests (17 test suites)
cargo test --workspace

# Run full Rust integration tests specifically
cargo test --test e2e_streaming_auth_test
cargo test --test e2e_widget_ast_test
cargo test --test e2e_subtitles_test
cargo test --test e2e_events_telemetry_test
cargo test --test e2e_web_serving_test

# Run frontend tests (Vitest)
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

All protected endpoints require an `Authorization: Bearer <token>` header, acquired via PIN authentication.

### Authentication & Profiles
- `POST /api/v1/auth/pin` (or `/profile-pin`) — Authenticate with 4-digit PIN. Returns JWT token and user info.
- `GET /api/v1/auth/me` — Retrieve authenticated user profile.

### Video Streaming & Artwork
- `GET /api/v1/stream/:item_id` — Zero-copy HTTP 206 range streaming (supports Bearer header and `?token=` query param).
- `GET /api/v1/artwork/:item_id/poster` — Stream cached poster artwork (`image/jpeg`, `image/png`, `image/webp`).
- `GET /api/v1/artwork/:item_id/backdrop` — Stream cached backdrop artwork.

### Declarative Screens & Widget AST
- `GET /api/v1/screens` — List available screen layouts (`home`, `movies`, `shows`).
- `GET /api/v1/screens/:screen_id` — Retrieve fully hydrated or unhydrated screen AST.
- `GET /api/v1/widgets/:widget_id/data?offset=0&limit=20` — Fetch paginated items for a specific widget query.
- `GET /api/v1/items/:item_id/details` — Detailed metadata payload for modal inspection.

### Playback & Scrobbling
- `POST /api/v1/playback/sessions` — Start a new playback tracking session (`{"media_item_id": 42}`).
- `POST /api/v1/playback/:session_id/progress` — 10-second heartbeat progress update (`{"position_seconds": 120}`).
- `DELETE /api/v1/playback/sessions/:session_id` — Close active playback session.
- `GET /api/v1/playback/states/:item_id` — Fetch user's saved playback position and watch status.
- `GET /api/v1/playback/continue-watching` — List items currently in progress for Continue Watching carousel.

### Subtitle Management
- `GET /api/v1/items/:item_id/subtitles` — List available subtitle tracks for an item.
- `GET /api/v1/subtitles/:subtitle_id/stream.vtt` — Public WebVTT stream (on-the-fly SRT $\to$ WebVTT conversion with disk caching).
- `GET /api/v1/subtitles/:item_id/search?languages=en,ar` — Search online subtitles via OpenSubtitles.
- `POST /api/v1/subtitles/:item_id/download` — Download online subtitle track and register in database.
- `DELETE /api/v1/subtitles/:subtitle_id` — Delete subtitle track and purge cache.

### Real-Time Events & Telemetry
- `GET /api/v1/events` — Public Server-Sent Events (SSE) stream (`library:updated`, `session:synced`, `subtitle:downloaded`, `system:telemetry`).
- `GET /api/v1/system/telemetry` — Admin-only instantaneous metrics snapshot (`active_sessions`, `rss_memory_bytes`, `db_size_bytes`, `wal_size_bytes`).

---

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.
