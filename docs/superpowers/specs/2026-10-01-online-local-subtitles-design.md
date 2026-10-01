# Design Specification: Kadr Milestone 4 — Online & Local Subtitle Engine

**Document Status:** Approved  
**Date:** 2026-10-01  
**Target Milestone:** Milestone 4 (Online & Local Subtitle Engine)  
**Author:** Pair Programming Agent & User  

---

## 1. System Overview & Scope

Kadr (كادر) is a lightweight, zero-bloat, direct-play self-hosted media server written in Rust with an embedded SQLite database. This design specification covers **Milestone 4**: the **Online & Local Subtitle Engine**.

Milestone 4 introduces comprehensive subtitle discovery, storage, on-the-fly WebVTT conversion, caching, and on-demand online fetching via OpenSubtitles.com REST API v1. Client video players (web, TV, mobile) receive standardized WebVTT (`text/vtt; charset=utf-8`) streams for all subtitle formats (SRT, VTT, ASS) with instantaneous $< 5\text{ ms}$ response times through disk caching, while local sidecar discovery functions 100% offline with zero external C/native dependencies.

### 1.1 Non-Goals for Milestone 4
- Bitmap subtitle OCR (e.g. converting image-based DVD VobSub / Blu-ray PGS to text). Bitmap subtitles are flagged as unsupported text tracks.
- Transcoding / burning subtitles into video streams via child FFmpeg processes (maintaining Kadr's direct-play HTTP 206 architecture).
- Full-Text Search (FTS5) library search queries (deferred to a dedicated catalog search milestone).

### 1.2 Performance & System Constraints
- **RSS Memory Budget:** $\le 30\text{ MB}$ under idle and active subtitle streaming/conversion workloads.
- **Latency / Response Budget:** $< 5\text{ ms}$ for cached WebVTT streams; $< 50\text{ ms}$ for on-the-fly SubRip (SRT) to WebVTT conversion.
- **Zero-External Runtime Dependencies Baseline:** Pure Rust implementation compilable against `x86_64-unknown-linux-musl`.
- **Database Safety:** All SQLite operations maintain WAL mode, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.

---

## 2. Architecture & Crate Boundaries

Milestone 4 extends the workspace across all four crates:

```
crates/
├── kadr-core/
│   └── src/
│       ├── subtitles.rs         # [NEW] SubtitleTrack, SubtitleSource, SubtitleFormat, OnlineSubtitleMatch
│       └── subtitles/
│           └── transcoder.rs    # [NEW] Pure-Rust SRT -> WebVTT stream parser & timecode normalizer
├── kadr-storage/
│   └── src/
│       ├── migrations/
│       │   └── 004_subtitles.sql# [NEW] media_subtitles table with cascade delete & language indexes
│       └── repos/
│           └── subtitle_repo.rs # [NEW] SubtitleRepository (create, batch_insert, find, delete, set_default)
├── kadr-ingest/
│   └── src/
│       ├── sidecar.rs           # [EXTENDED] Sidecar subtitle scanner (.srt, .vtt, .ass, language tag parser)
│       └── probe.rs             # [EXTENDED] Extraction of embedded container subtitle streams
└── kadr-server/
    └── src/
        ├── subtitles/
        │   ├── opensubtitles.rs # [NEW] Async OpenSubtitles REST API v1 client
        │   └── service.rs       # [NEW] Subtitle delivery service with disk cache (data/subtitles_cache/)
        ├── api/
        │   └── subtitle_routes.rs# [NEW] REST routes (/api/v1/items/:id/subtitles, /api/v1/subtitles/:id/stream.vtt, search, download)
        └── main.rs              # [EXTENDED] Mount subtitle routes and configure OpenSubtitles
```

---

## 3. Data Models & WebVTT Transcoder (`kadr-core`)

All models are serializable and derive `Clone`, `Debug`, `PartialEq`, `Eq`.

### 3.1 Domain Models (`kadr_core::subtitles`)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleSource {
    Sidecar,    // Found alongside media on disk (.srt, .vtt)
    Embedded,   // Found inside media container (MKV/MP4 stream)
    Downloaded, // Downloaded from OpenSubtitles
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFormat {
    Srt,
    Vtt,
    Ass,
    Sub,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub id: i64,
    pub media_item_id: i64,
    pub source: SubtitleSource,
    pub language: String,              // ISO 639-1/2 (e.g. "eng", "ara", "fre", "und")
    pub title: Option<String>,          // e.g. "English [SDH]", "Arabic Full"
    pub format: SubtitleFormat,
    pub file_path: Option<String>,      // Disk path for sidecars or downloaded files
    pub stream_index: Option<u32>,      // Container stream index for embedded tracks
    pub is_default: bool,
    pub is_forced: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineSubtitleMatch {
    pub id: String,                    // OpenSubtitles file ID
    pub language: String,              // e.g. "en", "ar"
    pub release_name: Option<String>,  // Release name/hash sync
    pub hearing_impaired: bool,
    pub format: String,                // "srt"
    pub download_count: u32,
    pub rating: Option<f32>,
}
```

### 3.2 Pure-Rust SubRip (SRT) to WebVTT Transcoder (`kadr_core::subtitles::transcoder`)

```rust
pub fn srt_to_webvtt(srt_content: &str) -> String {
    // 1. Emits "WEBVTT\n\n"
    // 2. Normalizes line endings (\r\n -> \n) and strips UTF-8 BOM (\u{FEFF})
    // 3. Parses timestamps: converts "00:01:23,456 --> 00:01:25,789" to "00:01:23.456 --> 00:01:25.789"
    // 4. Emits clean cue text preserving <i>, <b>, <u> formatting
}
```

---

## 4. Storage Engine & Subtitle Repository (`kadr-storage`)

### 4.1 Schema Migration `004_subtitles.sql`

```sql
CREATE TABLE media_subtitles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    source TEXT NOT NULL,          -- 'sidecar', 'embedded', 'downloaded'
    language TEXT NOT NULL,        -- 'eng', 'ara', 'fre', 'und'
    title TEXT,                    -- 'English [SDH]', etc.
    format TEXT NOT NULL,          -- 'srt', 'vtt', 'ass', 'sub'
    file_path TEXT,                -- Disk path for sidecars or downloaded files
    stream_index INTEGER,          -- Container stream index for embedded tracks
    is_default INTEGER NOT NULL DEFAULT 0,
    is_forced INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_subtitles_item ON media_subtitles(media_item_id);
CREATE INDEX idx_subtitles_lang ON media_subtitles(media_item_id, language);
```

### 4.2 `SubtitleRepository` API

```rust
impl SubtitleRepository {
    pub async fn create(&self, track: &SubtitleTrack) -> Result<i64, StorageError>;
    pub async fn batch_insert(&self, tracks: &[SubtitleTrack]) -> Result<(), StorageError>;
    pub async fn find_by_id(&self, id: i64) -> Result<Option<SubtitleTrack>, StorageError>;
    pub async fn find_by_media_item(&self, media_item_id: i64) -> Result<Vec<SubtitleTrack>, StorageError>;
    pub async fn delete(&self, id: i64) -> Result<bool, StorageError>;
    pub async fn set_default(&self, id: i64, media_item_id: i64) -> Result<(), StorageError>;
}
```

---

## 5. Ingestion Pipeline Extensions (`kadr-ingest`)

### 5.1 Sidecar Subtitle Discovery (`kadr_ingest::sidecar`)
- Scans containing directory and immediate `Subs/` or `Subtitles/` subfolder.
- Matches subtitle extensions: `.srt`, `.vtt`, `.ass`, `.sub`.
- Recognizes language and flag tags from filenames:
  - `<basename>.<lang>.srt` (e.g. `Movie.en.srt`, `Movie.ara.vtt`)
  - `<basename>.<lang>.forced.srt` (`is_forced = true`)
  - `<basename>.<lang>.default.srt` (`is_default = true`)
  - `<basename>.<lang>.sdh.srt` (`title = "... [SDH]"`)
  - `<basename>.srt` (`language = "und"`, `title = "Default Subtitle"`)

### 5.2 Container Subtitle Stream Probing (`kadr_ingest::probe`)
- Extends `TechnicalProber` to detect text subtitle tracks in containers (`subrip`, `mov_text`, `webvtt`, `ass`).
- Captures `stream_index`, language tag, and stream disposition flags.

---

## 6. Server Delivery Service & REST API (`kadr-server`)

### 6.1 Subtitle Delivery Service (`crates/kadr-server/src/subtitles/service.rs`)
- Manages disk cache directory at `<data_dir>/subtitles_cache/<subtitle_id>.vtt`.
- On request:
  1. Checks for cached `<id>.vtt`. If present, streams directly.
  2. If cache miss: loads source file (sidecar or downloaded), runs `srt_to_webvtt`, writes to cache file, and streams.
  3. Returns `Content-Type: text/vtt; charset=utf-8` and `Cache-Control: public, max-age=86400`.

### 6.2 OpenSubtitles.com Client (`crates/kadr-server/src/subtitles/opensubtitles.rs`)
- Configured via `kadr.toml`:
  ```toml
  [subtitles.opensubtitles]
  enabled = true
  api_key = "..."
  ```
- Implements asynchronous `search` and `download` against OpenSubtitles REST API v1.
- Gracefully handles missing API keys by returning empty match lists with a warning.

### 6.3 REST Endpoints (`/api/v1`)

| Method | Path | Auth | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/items/:item_id/subtitles` | `AuthUser` | Lists all subtitle tracks for a media item. |
| `GET` | `/api/v1/subtitles/:subtitle_id/stream.vtt` | Public / Token | Streams converted WebVTT file with cache headers. |
| `GET` | `/api/v1/subtitles/:item_id/search?languages=...` | `AuthUser` | Searches OpenSubtitles for online subtitle matches. |
| `POST` | `/api/v1/subtitles/:item_id/download` | `AuthUser` | Downloads an online subtitle, saves to disk, and indexes track. |
| `DELETE` | `/api/v1/subtitles/:subtitle_id` | `AuthUser` | Deletes a subtitle track and its cached file. |

---

## 7. Testing & Verification Strategy

1. **Unit Tests:**
   - SRT to WebVTT conversion, timestamp normalization, and cue tag handling (`crates/kadr-core/tests/subtitles_test.rs`).
   - SQLite migration 004 and `SubtitleRepository` CRUD operations (`crates/kadr-storage/tests/subtitle_repo_test.rs`).
   - Sidecar filename parser and language extractor (`crates/kadr-ingest/tests/sidecar_subtitles_test.rs`).
2. **Integration Tests:**
   - Subtitle listing and WebVTT streaming with cache generation and verification (`crates/kadr-server/tests/subtitle_routes_test.rs`).
   - OpenSubtitles mock client testing (search and download flows).
3. **End-to-End Test (`tests/e2e_subtitles_test.rs`):**
   - Ingest media with sidecar `.srt` and `.vtt` files $\to$ Authenticate user $\to$ Query `/api/v1/items/:id/subtitles` $\to$ Stream `/api/v1/subtitles/:id/stream.vtt` $\to$ Verify exact WebVTT syntax and subsequent $< 5\text{ ms}$ cache hit $\to$ Delete subtitle track.
