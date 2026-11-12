# Changelog

All notable changes to the Kadr media server will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.2.0-alpha] - 2026-11-12

### Added
- **Manual Ordered Playlists & Privacy-Isolated Storage**: Added SQLite migration `009_playlists.sql` (`playlists` and `playlist_items` tables with cascading deletes) and `PlaylistRepository` supporting per-user playlist ownership, server-wide public sharing (`is_public`), and fail-closed PIN-locked library isolation (`PlaylistSummary.locked_item_count` without leaking locked metadata or artwork, anti-probing `contains_queried_item: false` on `GET /api/v1/playlists?contains_item_id={id}`, tail-preserving `reorder_visible_items`, and `403 Forbidden` `LIBRARY_LOCKED` on locked item removal).
- **Playlists REST API, Batch Expansion, Widget AST & SSE**: Added `/api/v1/playlists` (`GET`, `POST` with optional 1-step `items` seeding), `/api/v1/playlists/{id}` (`GET`, `PATCH`, `DELETE`), `POST /api/v1/playlists/{id}/items`, `PUT /api/v1/playlists/{id}/items/order`, and `DELETE /api/v1/playlists/{id}/items/{media_item_id}` with batch expansion for ordered `item_ids`, TV shows/seasons (`show: { show_id, season_number }`), and sandboxed library folders (`folder: { library_id, path, recursive }`), plus `QueryMacro::PlaylistItems` (`playlist_items`) resolution with `GET /api/v1/widgets/{widget_id}/data` pagination, `playlist:updated` SSE events (`SystemEvent::PlaylistUpdated`), and `docs/CLIENT_GUIDE.md` reference recipes.
- **Playlists Web UI & Layout Studio Picker**: Added top-bar `Playlists` navigation (`PlaylistsScreen` splitting "My Playlists" and "Shared on Server" with 2x2 `PlaylistCoverMosaic` cards and `+N locked` badges), `PlaylistDetailView` and `PlaylistFormModal` (Play All, Shuffle, drag-and-drop and Up/Down reordering, inline editing, and deletion), `AddToPlaylistModal` wired into `SpotlightWidget`, `ItemDetailsModal` (movies, shows, seasons, and episodes), and `FolderBrowser` (recursive subfolder toggle and 1-step Create & Add), plus a playlist picker for `playlist_items` widgets in Layout Studio (`WidgetConfigModal`).
- **CinemaPlayer Multi-Track Playback Queue**: Extended `CinemaPlayer` and `PlayerControls` with multi-track queue playback (`queue`, `currentIndex`), automatic track advancement on `ended`, Previous/Next track controls, an interactive Queue popover (`1..N` track jump), and contextual return-to-previous-view navigation (`returnLabel`).
- **Seek Preview Thumbnails (Storyboard Trickplay)**: Added `StoryboardService`, `[storyboard]` configuration (`enabled`, `max_concurrent`, `interval_secs`, `tile_width`, `generate_on_scan`), storyboard manifest endpoint (`GET /api/v1/media/{item_id}/storyboard` returning `200 OK` or `202 Accepted` with `Retry-After: 5`), cached 10x10 JPEG sprite sheets (`GET /api/v1/media/{item_id}/storyboard/{sheet}`), and single-frame JPEG fallback (`GET /api/v1/media/{item_id}/frame?t=&w=`) with private-library lock enforcement (`403 Forbidden` `LIBRARY_LOCKED`), `ETag` / `If-None-Match` (`304 Not Modified`) and `Cache-Control: public, max-age=604800` headers, `has_video_stream` filtering, optional scan pre-generation (`generate_on_scan`), and ingest cache cleanup (`purge_for_path`).
- **Out-of-Process HTTP Plugin Framework**: Added SQLite migration `007_plugins.sql` (`plugins` table and `libraries.plugin_id`), `PluginService`, admin lifecycle routes (`/api/v1/admin/plugins`, `/api/v1/admin/plugins/{id}/approve`, `/enable`, `/disable`, and `DELETE /api/v1/admin/plugins/{id}?purge=`), scoped plugin publishing endpoints (`POST /api/v1/plugin/libraries`, `PUT /api/v1/plugin/libraries/{id}/items`, `DELETE /api/v1/plugin/libraries/{id}/items/{remote_id}`), playback resolution (`direct` `307 Temporary Redirect`, `proxy` with Range forwarding, and `transcode` via URL-backed HLS), capability-keyed descriptor caching, HMAC-derived per-plugin secrets, SSRF URL validation, per-plugin circuit breaker, 30-second health monitoring with `plugin:error` SSE events (`SystemEvent::PluginError`), `[plugins]` configuration (`enabled = false` by default), `docs/PLUGIN_GUIDE.md`, and `examples/plugin-hello`.
- **URL-Backed HLS Transcoding & Live Channel Playback**: Added `MediaType::LiveChannel` (`live_channel`), `PlaybackSource::Url`, `HlsSessionManager::get_or_create_session_for_source`, and `build_ffmpeg_hls_args_for_source` for transcoding remote plugin streams, alongside live-item session tracking (`is_live`) that maintains active session heartbeats without persisting watch-progress scrobbles.
- **Web UI Plugin Management & Persisted Settings**: Added SQLite migration `008_settings.sql` (`settings` table) and `SettingsRepository` so database-backed `[plugins]` settings (`GET`/`PUT /api/v1/admin/plugins/settings`, reserving plugin ID `settings`) override `kadr.toml` defaults, admin credential endpoints (`GET /api/v1/admin/plugins/{id}/connection`, `POST /api/v1/admin/plugins/{id}/token`), and the **Plugins** tab in `AdminDashboard` (`PluginsPanel`, `RegisterPluginForm`, `PluginList`, `ApprovePluginDialog`, `RemovePluginDialog`, `ConnectionDialog`).

### Fixed
- **Startup Deduplication Playlist Preservation**: Updated `MediaItemRepository::deduplicate_media_items` to migrate `playlist_items` rows from duplicate symlink entries onto canonical media items (`UPDATE OR IGNORE`) and re-compact dense `0..N-1` playlist positions without losing membership or ordering.
- **Storyboard Cache Pruning & In-Flight Cleanup**: Cleaned up temporary directories on mid-generation `ffmpeg` errors, released in-flight generation locks via an RAII guard (`InFlightGuard`), and pruned stale single-frame cache entries alongside outdated `source_hash` directories when media files change on disk.
- **Plugin SSRF & Admin UI Hardening**: Blocked IPv4-mapped and IPv6-compatible private addresses in the plugin URL SSRF guard, enforced single half-open probe concurrency on the plugin circuit breaker, keyed playback descriptor caches by `ClientCapabilities`, and hardened `PluginsPanel` against refresh races when disabling plugins.

## [v0.1.3-alpha] - 2026-11-07

### Fixed
- Allowed read-only access to `/home` in the systemd unit and added detailed filesystem error diagnostics to the folder browsing API and folder picker modal.
- Debian `postinst` now automatically restarts the `kadr` service if it is active during upgrades.

## [v0.1.2-alpha] - 2026-10-29

### Added
- **On-Demand HLS Transcoding & Universal Playback Fallback**: Added universal HTTP Live Streaming (HLS) fallback pipeline for Smart TVs (such as Samsung Tizen OS) and web clients encountering unsupported audio/video codecs or container formats (`MKV`, `AVI`, `DTS`, `TrueHD`, `HEVC 10-bit`, `AV1`).
- **Smart Stream Copy & Transcode Decision Engine**: Implemented `TranscodeDecisionEngine` and `FFmpegCommandBuilder` in `kadr-core` supporting zero-CPU pure remuxing (`-c:v copy -c:a copy`), audio-only transcoding (`-c:v copy -c:a aac`), and full video transcoding (`libx264`, `vaapi`, `nvenc`) with resolution downscaling.
- **HLS Session Manager & Inactivity Reaper**: Added `HlsSessionManager` in `kadr-server` to manage ephemeral `ffmpeg` child processes, session deduplication, concurrency throttling (`max_concurrent_transcodes`), stderr diagnostics draining, and a 15-second background reaper task to terminate idle transcode sessions and purge temporary segment directories.
- **HLS Streaming & Session Teardown API**: Added `GET /api/v1/media/:id/hls/master.m3u8`, `GET /api/v1/media/:id/hls/sessions/:session_id/segment_:seq.ts`, and `DELETE /api/v1/media/:id/hls/sessions/:session_id` endpoints with library unlock validation and bounded 64 KiB chunked delivery.
- **Permissive CORS Support**: Enabled permissive CORS layer on the HTTP API router to support cross-origin web clients, Smart TV apps, and local development environments.
- **Client Guide HLS Documentation**: Updated `docs/CLIENT_GUIDE.md` with codec capability inspection matrices, HLS fallback URL construction, and Samsung Tizen `AVPlay` integration examples.

### Fixed
- Preserved authentication query parameters (`token` and `unlock_tokens`) across rewritten segment URLs in HLS playlists for native players that do not forward custom HTTP headers on segment requests.

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

[v0.2.0-alpha]: https://github.com/BooDy/kadr/compare/v0.1.3-alpha...v0.2.0-alpha
[v0.1.3-alpha]: https://github.com/BooDy/kadr/compare/v0.1.2-alpha...v0.1.3-alpha
[v0.1.2-alpha]: https://github.com/BooDy/kadr/compare/v0.1.1-alpha...v0.1.2-alpha
[v0.1.1-alpha]: https://github.com/BooDy/kadr/compare/v0.1.0-alpha.2...v0.1.1-alpha
[v0.1.0-alpha.2]: https://github.com/BooDy/kadr/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[v0.1.0-alpha]: https://github.com/BooDy/kadr/releases/tag/v0.1.0-alpha
