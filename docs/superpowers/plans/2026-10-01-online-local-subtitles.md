# Milestone 4: Online & Local Subtitle Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Online & Local Subtitle Engine, providing automatic sidecar discovery, container stream probing, relational track cataloging in SQLite, pure-Rust on-the-fly SubRip to WebVTT conversion with disk caching, OpenSubtitles.com online search and download, and client streaming REST endpoints.

**Architecture:** Domain models and pure-Rust WebVTT transcoder in `kadr-core`, migration `004_subtitles.sql` and `SubtitleRepository` in `kadr-storage`, sidecar and container probing in `kadr-ingest`, OpenSubtitles client and disk-cached WebVTT delivery service in `kadr-server`, exposed over Axum REST endpoints (`/api/v1/subtitles`, `/api/v1/items/:id/subtitles`).

**Tech Stack:** Rust (2021 edition), Axum 0.8, SQLite with WAL mode (`deadpool-sqlite`, `rusqlite`), `reqwest` (with rustls-tls), `tokio`, `tracing`.

## Global Constraints
- RSS memory usage must remain <= 30 MB under idle and standard subtitle streaming/conversion workloads.
- Latency / response budget: < 5 ms for cached WebVTT streams; < 50 ms for on-the-fly SRT to WebVTT conversions.
- Zero-external runtime dependencies baseline (compilable against `x86_64-unknown-linux-musl`).
- All database operations in SQLite must run with WAL mode, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.
- Subtitle streaming (`/api/v1/subtitles/:id/stream.vtt`) must support unauthenticated and query-token requests with `Content-Type: text/vtt; charset=utf-8` and `Cache-Control: public, max-age=86400` so standard HTML5 `<track>` elements work without custom headers.
- OpenSubtitles API integration must gracefully handle missing API keys by returning empty match lists and warning logs without failing.

---

### Task 1: Domain Models & Pure-Rust WebVTT Transcoder (`kadr-core`)

**Files:**
- Create: `crates/kadr-core/src/subtitles.rs`
- Create: `crates/kadr-core/src/subtitles/transcoder.rs`
- Modify: `crates/kadr-core/src/lib.rs`
- Test: `crates/kadr-core/tests/subtitles_test.rs`

**Interfaces:**
- Produces: `SubtitleSource`, `SubtitleFormat`, `SubtitleTrack`, `OnlineSubtitleMatch`, `srt_to_webvtt(&str) -> String`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-core/tests/subtitles_test.rs` testing:
- SRT to WebVTT conversion with timestamp normalization (`00:01:23,456` $\to$ `00:01:23.456`).
- WebVTT header generation (`WEBVTT\n\n`).
- UTF-8 BOM removal (`\u{FEFF}`).
- Preservation of formatting tags (`<i>`, `<b>`).
- Multi-line cue handling and empty line normalization.
- Subtitle domain models serialization and deserialization.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-core --test subtitles_test`
Expected: Compilation failure because `subtitles` module does not exist.

- [ ] **Step 3: Implement domain models and transcoder**
- Create `crates/kadr-core/src/subtitles.rs` and `crates/kadr-core/src/subtitles/transcoder.rs`.
- Implement `srt_to_webvtt(srt: &str) -> String`.
- Expose `pub mod subtitles;` in `crates/kadr-core/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-core --test subtitles_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(core): implement subtitle domain models and pure-Rust SRT to WebVTT transcoder`

---

### Task 2: Schema Migration 004 & `SubtitleRepository` (`kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/migrations/004_subtitles.sql`
- Modify: `crates/kadr-storage/src/migrations.rs`
- Create: `crates/kadr-storage/src/repos/subtitle_repo.rs`
- Modify: `crates/kadr-storage/src/repos/mod.rs`
- Modify: `crates/kadr-storage/src/lib.rs`
- Test: `crates/kadr-storage/tests/subtitle_repo_test.rs`

**Interfaces:**
- Consumes: `SubtitleTrack`, `SubtitleSource`, `SubtitleFormat` from `kadr-core::subtitles`
- Produces: `SubtitleRepository` with `create`, `batch_insert`, `find_by_id`, `find_by_media_item`, `delete`, `set_default`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-storage/tests/subtitle_repo_test.rs` testing:
- Migration 004 applies cleanly.
- `create` inserts subtitle track and returns primary key ID.
- `batch_insert` inserts multiple tracks.
- `find_by_media_item` lists all tracks for a media item.
- Foreign key cascade delete (deleting `MediaItem` automatically purges associated subtitle rows).
- `set_default` unsets previous default and sets new default.
- `delete` removes individual track.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-storage --test subtitle_repo_test`
Expected: Compilation failure due to missing `SubtitleRepository`.

- [ ] **Step 3: Implement migration 004 and `SubtitleRepository`**
- Create `crates/kadr-storage/src/migrations/004_subtitles.sql`.
- Register in `crates/kadr-storage/src/migrations.rs`.
- Implement `crates/kadr-storage/src/repos/subtitle_repo.rs`.
- Expose in `crates/kadr-storage/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-storage --test subtitle_repo_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(storage): add migration 004 and SubtitleRepository`

---

### Task 3: Sidecar Subtitle Discovery & Container Track Inspection (`kadr-ingest`)

**Files:**
- Create: `crates/kadr-ingest/src/sidecar_subtitles.rs`
- Modify: `crates/kadr-ingest/src/lib.rs`
- Modify: `crates/kadr-ingest/src/worker.rs`
- Test: `crates/kadr-ingest/tests/sidecar_subtitles_test.rs`

**Interfaces:**
- Consumes: `SubtitleTrack`, `SubtitleRepository`
- Produces: `scan_sidecar_subtitles(&Path) -> Vec<SubtitleTrack>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-ingest/tests/sidecar_subtitles_test.rs` testing:
- Detection of `.srt`, `.vtt`, `.ass`, `.sub` files.
- Extraction of language tags: `movie.en.srt` -> `eng`, `movie.ara.vtt` -> `ara`, `movie.srt` -> `und`.
- Extraction of flags: `movie.en.forced.srt` -> `is_forced = true`, `movie.en.default.srt` -> `is_default = true`.
- Scanning immediate `Subs/` or `Subtitles/` directory.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-ingest --test sidecar_subtitles_test`
Expected: Compilation failure because `sidecar_subtitles` does not exist.

- [ ] **Step 3: Implement sidecar scanner and worker integration**
- Implement `crates/kadr-ingest/src/sidecar_subtitles.rs`.
- Wire `scan_sidecar_subtitles` in `worker.rs` to insert discovered tracks using `SubtitleRepository::batch_insert`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-ingest --test sidecar_subtitles_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(ingest): implement sidecar subtitle scanner and worker persistence`

---

### Task 4: Subtitle Delivery Service with On-the-Fly WebVTT Caching (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/subtitles/service.rs`
- Create: `crates/kadr-server/src/subtitles/mod.rs`
- Modify: `crates/kadr-server/src/lib.rs`
- Test: `crates/kadr-server/tests/subtitle_service_test.rs`

**Interfaces:**
- Consumes: `SubtitleRepository`, `MediaItemRepository`, `srt_to_webvtt`
- Produces: `SubtitleDeliveryService::new(cache_dir, subtitle_repo, media_repo)`, `get_webvtt_stream(&self, subtitle_id: i64) -> Result<PathBuf, SubtitleError>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/subtitle_service_test.rs` testing:
- On cache miss: converts `.srt` sidecar to `.vtt`, writes to cache directory, returns path to cached `.vtt`.
- On cache hit: immediately returns path to cached `.vtt` in < 5 ms.
- If source is already `.vtt`, returns path or copies directly.
- Handles missing source file cleanly with error.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test subtitle_service_test`
Expected: Compilation failure because `subtitles` module does not exist.

- [ ] **Step 3: Implement SubtitleDeliveryService**
Implement `crates/kadr-server/src/subtitles/service.rs` and `mod.rs`. Expose in `crates/kadr-server/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test subtitle_service_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement SubtitleDeliveryService with on-the-fly WebVTT caching`

---

### Task 5: OpenSubtitles.com Online REST API Client (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/subtitles/opensubtitles.rs`
- Modify: `crates/kadr-server/src/subtitles/mod.rs`
- Modify: `crates/kadr-server/Cargo.toml` (add `reqwest` with `rustls-tls` features)
- Test: `crates/kadr-server/tests/opensubtitles_client_test.rs`

**Interfaces:**
- Produces: `OpenSubtitlesClient::new(api_key: Option<String>)`, `search(&self, query: &str, year: Option<u32>, languages: &[String]) -> Result<Vec<OnlineSubtitleMatch>, ...>`, `download(&self, file_id: &str) -> Result<Vec<u8>, ...>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/opensubtitles_client_test.rs` testing:
- Client initialization with and without API key.
- Graceful empty results when unconfigured.
- Mock server responses for search and download.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test opensubtitles_client_test`
Expected: Compilation failure.

- [ ] **Step 3: Implement OpenSubtitlesClient**
Implement `crates/kadr-server/src/subtitles/opensubtitles.rs`. Add dependencies to `Cargo.toml`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test opensubtitles_client_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement OpenSubtitles REST API client`

---

### Task 6: Subtitle Management & Streaming REST API Routes (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/subtitle_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Test: `crates/kadr-server/tests/subtitle_routes_test.rs`

**Interfaces:**
- Mounts in Axum:
  - `GET /api/v1/items/:item_id/subtitles`
  - `GET /api/v1/subtitles/:subtitle_id/stream.vtt`
  - `GET /api/v1/subtitles/:item_id/search`
  - `POST /api/v1/subtitles/:item_id/download`
  - `DELETE /api/v1/subtitles/:subtitle_id`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/subtitle_routes_test.rs` testing:
- Authenticated listing of subtitles for a media item.
- Public / Token streaming of WebVTT subtitle track with `Content-Type: text/vtt; charset=utf-8` and `Cache-Control: public, max-age=86400`.
- Online search endpoint behavior.
- Download endpoint saving track and registering in database.
- Delete endpoint removing track.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test subtitle_routes_test`
Expected: Compilation failure due to missing routes.

- [ ] **Step 3: Implement subtitle routes**
Implement `subtitle_routes.rs` and wire into `crates/kadr-server/src/api/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test subtitle_routes_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement subtitle streaming and management REST API routes`

---

### Task 7: Server Bootstrap Assembly & End-to-End Milestone 4 Integration Test

**Files:**
- Modify: `crates/kadr-server/src/main.rs`
- Modify: `crates/kadr-server/Cargo.toml`
- Create: `tests/e2e_subtitles_test.rs`

**Interfaces:**
- Consumes: All Milestone 4 components across all crates
- Produces: Assembled server with subtitle engine and end-to-end integration test

- [ ] **Step 1: Write the end-to-end integration test**
Create `tests/e2e_subtitles_test.rs` exercising the complete Milestone 4 user journey:
1. Ingest movie media with `.srt` sidecar on disk.
2. Authenticate admin user via PIN `1234`.
3. Query `GET /api/v1/items/:id/subtitles` -> assert sidecar track is discovered.
4. Stream `GET /api/v1/subtitles/:id/stream.vtt` -> verify HTTP 200, WebVTT header, converted timestamps (`.` instead of `,`), and cache headers.
5. Stream again -> verify instant response from disk cache.
6. Verify online search endpoint with mock / unconfigured response.
7. Delete subtitle track -> verify 200 OK and purged from listing.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test e2e_subtitles_test`
Expected: Compilation/routing failure.

- [ ] **Step 3: Wire SubtitleDeliveryService and OpenSubtitles in `main.rs`**
Update `crates/kadr-server/src/main.rs` to initialize and pass subtitle dependencies to `create_router_with_layout`. Update `crates/kadr-server/Cargo.toml` with `[[test]]` targeting `../../tests/e2e_subtitles_test.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test e2e_subtitles_test`
Expected: PASS

- [ ] **Step 5: Run full workspace verification**
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: All tests pass, 0 clippy warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): assemble subtitle engine into main router and add Milestone 4 E2E test`
