# Changelog

All notable changes to the Kadr media server will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.1.1-alpha] - 2026-10-07

### Added
- **Direct Folder Navigation**: Added "Browse Folder" action button in `ItemDetailsModal` enabling instant navigation to the containing directory in `FolderBrowser`, with cross-library switching and private library PIN unlock handling.
- **Client Developer Guide**: Created comprehensive reference guide (`docs/CLIENT_GUIDE.md`) detailing autodiscovery, authentication, declarative layout ASTs, video streaming, scrobbling lifecycle, subtitles, and real-time SSE events.
- **Image Viewing & Slideshow**: Integrated image discovery (`jpg`, `png`, `webp`, `gif`, `avif`, `bmp`) and fullscreen lightbox viewer with 4-second auto-advancing slideshow in Folder Browser.
- **Series & Seasons Grouping**: Added TV episode filename and folder cue parsing (`S01E02`), parent show aggregation, season tabs selector, and 1-click episode playback.
- **Video Thumbnail Extraction Engine**: Pure-Rust background thumbnail extractor during library scans and on-demand fallback routes (`/api/v1/thumbnails/videos/:id`).
- **Network Autodiscovery & Server Identity**: Added unauthenticated `/api/v1/discovery` endpoint and persistent server identity configuration in Admin Dashboard.
- **Hierarchical Folder Browser**: Added sandboxed folder browser (`/api/v1/libraries/:id/folders`) with breadcrumb navigation and view mode switcher (Catalog vs Folders).
- **In-Player Live Subtitle Search & Download**: Integrated OpenSubtitles search and automatic on-the-fly download directly inside CinemaPlayer.
- **Advanced Widget Filters**: Added query filters (exclude private, exclude libraries, exclude genres, max age days, rating filters) configurable in Layout Studio.
- **Interactive Layout Studio**: Built declarative AST visual layout builder with device viewports (TV 16:9, Tablet 4:3, Mobile 9:16), widget inspector, and custom screen persistence.
- **Multi-Path Libraries**: Added support for media libraries spanning multiple filesystem paths with a server-side directory picker modal.
- **Private Libraries with PIN Protection**: Added 4-digit PIN protection, database query isolation, and HMAC unlock tokens for sensitive media libraries.
- **Universal Linux Installer & Upgrader**: Added one-line online installer script (`scripts/install.sh` / root `install.sh`) supporting automated installation and in-place zero-downtime upgrades across all distributions.
- **Comprehensive Upgrade Documentation**: Added dedicated upgrade guides for Debian (`.deb`), automated script, and manual tarballs in `README.md`.
- **Inline Library Renaming**: Added inline name editing with pencil action on library cards in Admin Dashboard.

### Changed & Improved
- Enhanced standalone tarball installer (`packaging/scripts/install.sh`) with active service detection and in-place upgrade handling.
- Overhauled client styling to conform strictly to `theme.md` dark cinema palette and 10-foot TV UI focus rings (`focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`).
- Optimized SQLite WAL query execution with filter pushdown and prevented duplicate item ingestion across symlink traversals.
- Streamlined top navigation tabs and unified Admin Dashboard embedding Layout Studio and Telemetry into sub-tabs.

### Fixed
- Enforced image and folder sandboxing, canonical path checks, and path traversal (`..`) prevention on streaming endpoints.
- Synchronized native DOM `fullscreenchange` events in media viewers.
- Fixed player and viewer HUD auto-hide timeouts resetting on keyboard interactions.
- Resolved mutual startup deadlock between filesystem watcher and ingestion worker.

## [v0.1.0-alpha.2] - 2026-09-20

### Added
- Library management interface and admin configuration screen in Web UI.
- Configurable default server port (8492) and environment variable overrides.
- Automated Debian package generation (`.deb`) and Linux standalone archive packaging.

## [v0.1.0-alpha] - 2026-09-15

### Added
- Initial alpha release of Kadr media server core engine in pure Rust.
- Embedded SQLite storage engine in WAL mode with connection pooling.
- Axum-based HTTP server with zero-copy HTTP 206 Direct Play video streaming.
- Declarative Widget AST layout engine powering responsive Web UI.
- Local sidecar subtitle discovery, streaming SRT to WebVTT conversion, and OpenSubtitles API integration.
- Bounded Server-Sent Events (SSE) bus for system telemetry and realtime updates.
- Profile-based PIN authentication and JWT session management.

[v0.1.1-alpha]: https://github.com/BooDy/kadr/compare/v0.1.0-alpha.2...v0.1.1-alpha
[v0.1.0-alpha.2]: https://github.com/BooDy/kadr/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[v0.1.0-alpha]: https://github.com/BooDy/kadr/releases/tag/v0.1.0-alpha
