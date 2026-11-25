# Changelog

All notable changes to the Kadr media server will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.4.0] - 2026-11-25

### Added
- **Declarative Filter AST, SQL Compiler & Live Studio Rule Builder**: Added recursive `FilterNode` boolean trees (`AND`, `OR`, `NOT` up to depth `6` and `64` conditions) spanning `library_id`, `item_type`, `added_at`/`released_at`/`last_watched_at` epoch & relative sliding windows, `duration_seconds`, technical attributes (`resolution`, `video_codec`, `audio_codec`, `audio_channels`, `container`), rich metadata (`country`, `audio_language`, `director`, `writer`, `studio`, `genres`, `tags`, `rating`, `release_year`), `watch_state` (including dynamic percentage ranges), and `is_favorite`, compiled into parameterized SQLite prepared statements backed by expression indexes (`014_filter_ast_indexes.sql`), alongside `QueryMacro::CustomQuery` (`custom_query`), `GET /api/v1/catalog/facets`, stateless `POST /api/v1/widgets/preview`, and the visual `RuleBuilderEditor` + live widget previews in Layout Studio.
- **Per-User Favorites / Watchlist & Dedicated `Favorites` View**: Added SQLite migration `015_user_favorites.sql` (`user_favorites` partitioned by `user_id` with cascading deletes), `FavoritesRepository`, `GET`/`PUT`/`DELETE /api/v1/favorites` and `/api/v1/favorites/{item_id}`, zero-extra-roundtrip `CardViewModel.is_favorite` batch hydration, `QueryMacro::Favorites` (`favorites`) with a factory-default `Home` `"My Favorites"` rail, `FilterCondition::IsFavorite`, `favorite:updated` SSE events (`SystemEvent::FavoriteUpdated`), startup symlink-deduplication favorite preservation, optimistic `FavoriteButton` heart toggles across all browse cards, spotlight banners, dense rows, and item details, and a top-bar `FavoritesScreen` with media-type filter pills, sort controls, pagination, and `Play All` / `Shuffle` queue playback.
- **Per-User & Per-Device Scoped Layouts, `time_window` Shelves & `DenseList` Widget**: Added SQLite migration `016_scoped_layouts.sql` (`layout_overrides` with `ON DELETE CASCADE`) and `LayoutOverrideRepository`, in-memory 4-tier fallback resolution (`(user_id, target_device)` $\to$ `(user_id, all)` $\to$ `(global, target_device)` $\to$ `(global, all)` across `all`, `tv`, `web`, and `mobile` with zero per-request SQLite queries), client-clock `TimeWindow` shelf scheduling (`start`/`end` `"HH:MM"` supporting both same-day and overnight windows via `?local_time=` / `X-Kadr-Local-Time` and skipping SQL hydration on inactive shelves), `WidgetNode::DenseList` (`dense_list`) + `CardViewModel.duration_seconds` + `<DenseListWidget />` compact cinema rows, personal `LayoutStudio` access via top-bar `Studio` for `Standard` profiles (`scope_user_id=me`) alongside multi-profile/global scope authoring and `Simulate Time` previewing for `Admin` profiles, and scoped `layout:changed` SSE metadata.
- **Multi-Audio Stream Probing, Manual Watch-State API & Web `CinemaPlayer` Parity**: Added SQLite migration `013_audio_tracks.sql` persisting `ffprobe` multi-audio stream metadata across rescans, `GET /api/v1/media/{id}/audio-tracks` (with HLS codec selection keyed to the requested `audio_track`), audio-only HLS transcoding (`VideoAction::None` / `video_codec=none` / `-vn`) for `music` and `audiobook` streams, manual single and batch watch-state endpoints (`PUT /api/v1/playback/items/{id}/watch-state`, `PUT /api/v1/playback/watch-states`), dynamic `hls.js` fallback (`useHlsPlayback`) with seek timeline offsets and WebVTT cue sync, in-player multi-audio track switching and 10×10 storyboard seek-bar hover previews in `CinemaPlayer`, and single / season / series Mark Watched controls across catalog cards and `ItemDetailsModal`.

### Fixed
- **Canonical Library Screen Scoping, HLS Session Cleanup & Browse Race Hardening**: Unified canonical library screen lookup across `/api/v1/screens/{id}`, `/api/v1/widgets/{id}/data`, and `/api/v1/widgets/preview`, extended library rename/deletion synchronization across `Carousel`, `DenseList`, `HeroBanner`, and scoped overrides, preserved SQLite AST index scans on case-folded filters, preserved resume offsets and Safari session teardown across HLS fallbacks, and prevented `BrowseScreen` from hanging in a loading state when `refreshHome` preempts an initial `fetchScreen`.

## [v0.3.0-alpha] - 2026-11-17

### Added
- **Git-Based Plugin Installation & Process Supervision**: Added SQLite migration `010_managed_plugins.sql` (`install_kind`, `repo_url`, `forge_kind`, `installed_tag`), GitHub, GitLab v4, and Gitea/Forgejo v1 release asset discovery (`* -<os>-<arch>.tar.gz` with optional `checksums.txt`/`SHA256SUMS` verification and redirect-hop SSRF validation), a pure-Rust safe bundle unpacker (`flate2` `rust_backend` + `tar` with zero C dependencies, rejecting path traversal/symlinks/hardlinks/FIFOs/device nodes, enforcing `100 MiB` compressed / `250 MiB` uncompressed caps, preserving static `web/dist` assets, and staging binaries in `pending` without executing unapproved code), `PluginSupervisor` child-process lifecycle management (`127.0.0.1:<ephemeral_port>`, `Command::env_clear()` environment isolation, `/health` polling, exponential crash backoff, startup auto-launch, and graceful `SIGTERM` $\to$ `SIGKILL` shutdown), `POST /api/v1/admin/plugins/install` and `POST /api/v1/admin/plugins/{id}/update` (re-entering `pending` if permissions expand), and Admin Web UI controls (`Install from Git` vs `External URL`, `Managed` badge, release tag display, and 1-click updates).
- **Global SQLite FTS5 Search & Instant Search Modal**: Added SQLite migration `011_search_fts.sql` (`media_items_fts` virtual table with `unicode61 remove_diacritics 2` tokenization and `AFTER INSERT`/`UPDATE`/`DELETE` synchronization triggers indexing titles, series/episode `SxxExx` tags, overviews, cast/directors/writers, genres/tags/studio/country, filenames, and release years), `SearchRepository` with BM25 column-weighted ranking, an FTS5 query sanitizer supporting arbitrary punctuation, operators, and non-Latin/Arabic scripts without syntax errors, fail-closed private PIN-locked library isolation (`UnlockedLibraries`), `GET /api/v1/search?q=&item_type=&library_id=&limit=`, and a keyboard-driven `GlobalSearchModal` (`Ctrl+K`, `Cmd+K`, and `/` outside editable inputs) in the Web UI.
- **Inline Metadata & Artwork Editor + Opt-In TMDb Scraper**: Added SQLite migration `012_metadata_overrides.sql` (`media_items.locked_fields`) protecting manual and TMDb-applied metadata overrides across filesystem `upsert_batch` rescans (`reset_locks: true` to restore scanner defaults), `PATCH /api/v1/items/{id}/metadata` with SSRF-guarded local poster/backdrop caching (`<data_dir>/artwork/`, `15 MiB` cap, JPEG/PNG/WebP magic-byte verification), opt-in `TmdbClient` supporting both TMDb v3 API keys and v4 Bearer Read Access Tokens (`GET`/`PUT /api/v1/admin/settings/metadata` with masked `api_key_hint`, `GET /api/v1/items/{id}/tmdb/search`, `POST /api/v1/items/{id}/tmdb/apply`, `POST /api/v1/admin/libraries/{id}/tmdb/enrich`, and optional `auto_fetch_on_scan` ingest enrichment), plus `MetadataEditorModal`, `IdentifyTmdbModal`, and the `Metadata & TMDb` Admin Dashboard tab.

## [v0.2.1-alpha] - 2026-11-16

### Added
- **Embedded Container Subtitle Detection & WebVTT Extraction** (`4469846`, `86b9e1f`): Added `ffprobe` text subtitle stream discovery (`subrip`/`srt`, `text`, `ass`/`ssa`, `webvtt`/`vtt`, and `mov_text`/`tx3g` with generic muxer handler-name filtering including `SubtitleHandler`, `sbtl`, and Apple `Core Media Subtitle`, while ignoring bitmap subtitle streams), `SubtitleSource::Embedded` with `stream_index` metadata (`SubtitleTrack` / `SubtitleTrackResponse`), `SubtitleDeliveryService` on-demand FFmpeg WebVTT extraction (`-map 0:{stream_index} -c:s webvtt -f webvtt`) and `.ass`/`.ssa` sidecar WebVTT conversion with atomic temp-file caching and per-track singleflight deduplication locks, lazy backfill probing on `GET /api/v1/items/{item_id}/subtitles` (`list_or_probe_subtitles`) for pre-existing catalog items, and bounded `GET /api/v1/subtitles/{subtitle_id}/stream.vtt` streaming.
- **Unified & Expanded Video Container Support** (`86b9e1f`): Added `kadr_core::video_formats` (`VIDEO_EXTENSIONS`, `is_video_extension`, `is_video_path`, `video_mime_for_extension`) expanding supported video containers to **`mkv`, `mp4`, `m4v`, `mov`, `webm`, `avi`, `ts`, `m2ts`, `mts`, `3gp`, and `3g2`** across filename/episode parsing, live filesystem watching, sidecar subtitle matching, 256-byte magic-byte pure-Rust container inspection (`EBML` `mkv`/`webm`, ISO BMFF `ftyp` `mp4`/`m4v`/`mov`/`3gp`/`3g2`, `RIFF` `avi`, 188-byte MPEG-TS `ts`, 192-byte BDAV `m2ts`), folder browsing, playlist batch expansion, HTTP byte-range MIME resolution, and embedded subtitle probing.

### Fixed
- **SQLite Pool WAL Initialization & Concurrent Screen Resolution Test Stability**: Optimized `kadr_storage::pool::create_pool` to set `PRAGMA busy_timeout = 5000` first on every pooled connection and initialize persistent database-level `PRAGMA journal_mode = WAL` once per file-backed pool via `OnceCell` instead of acquiring write locks on every new pooled reader slot, and hardened `test_concurrent_home_screen_resolution` (plus cached WebVTT fast-path checks in `subtitle_service_test`) to exercise multi-connection WAL bursts without debug-build wall-clock flakiness under parallel workspace test load.
- **Storyboard Route Error Sizing** (`3533c28`): Boxed `axum::response::Response` errors in `storyboard_routes::load_and_authorize_item` to eliminate oversized `Result` stack frames under strict Clippy lints.

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

[v0.4.0]: https://github.com/BooDy/kadr/compare/v0.3.0-alpha...v0.4.0
[v0.3.0-alpha]: https://github.com/BooDy/kadr/compare/v0.2.1-alpha...v0.3.0-alpha
[v0.2.1-alpha]: https://github.com/BooDy/kadr/compare/v0.2.0-alpha...v0.2.1-alpha
[v0.2.0-alpha]: https://github.com/BooDy/kadr/compare/v0.1.3-alpha...v0.2.0-alpha
[v0.1.3-alpha]: https://github.com/BooDy/kadr/compare/v0.1.2-alpha...v0.1.3-alpha
[v0.1.2-alpha]: https://github.com/BooDy/kadr/compare/v0.1.1-alpha...v0.1.2-alpha
[v0.1.1-alpha]: https://github.com/BooDy/kadr/compare/v0.1.0-alpha.2...v0.1.1-alpha
[v0.1.0-alpha.2]: https://github.com/BooDy/kadr/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[v0.1.0-alpha]: https://github.com/BooDy/kadr/releases/tag/v0.1.0-alpha
