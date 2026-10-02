# Milestone 5B Design Spec: Management Web App & Layout Studio

**Date:** 2026-10-02  
**Status:** Approved  
**Author:** Antigravity Team  

---

## 1. Overview & Objectives

Milestone 5B delivers the official web client for Kadr: a unified, responsive Single-Page Application (SPA) providing media browsing, cinema-grade HTML5 video playback, real-time server telemetry, and a visual layout studio.

### Key Capabilities:
1. **PIN Authentication & Profile Switcher:** 4-digit PIN keypad modal, JWT storage in `localStorage`, and authenticated API client.
2. **Declarative Media Browser (`/browse`):** Native client rendering of Kadr's Declarative Widget AST (`Spotlight`, `Carousel`, `Grid`) with badges (`4K`, `NEW`, `RESUME` progress bar), artwork streaming (`poster`, `backdrop`), and rich item inspection modal with subtitle management.
3. **Cinema Player (`/player/:itemId`):** HTML5 video player with HTTP 206 streaming, resume position fetching, 10-second heartbeat progress reporting, native WebVTT subtitle track rendering with language switcher, and responsive keyboard controls.
4. **Layout Studio (`/studio`):** Visual preview dashboard for screen ASTs with responsive device frame toggles (TV, Tablet, Mobile) and widget node toggles.
5. **Server Telemetry & Event Monitor (`/telemetry`):** Real-time resource metrics dashboard (process RSS memory, SQLite DB/WAL sizes, active streaming sessions) and live Server-Sent Events (SSE) feed from `/api/v1/events`.
6. **Backend Static File Serving:** Axum server integration using `tower-http::services::ServeDir` serving `web/dist` with client-side SPA routing fallback to `index.html`.

---

## 2. Technical Stack & Architecture

### 2.1 Frontend Project (`web/`)
- **Build Tool:** Vite 6 + TypeScript
- **UI Framework:** React 19
- **Styling:** Tailwind CSS (Dark cinema theme: Slate/Zinc dark palette, high-contrast accents)
- **Icons:** Lucide React
- **Router:** Lightweight Hash or History routing for clean SPA navigation
- **API Client:** Lightweight typed fetch wrapper attaching `Authorization: Bearer <token>` and handling 401 unauthenticated redirects

### 2.2 Backend Integration (`crates/kadr-server`)
- Mount `ServeDir::new("web/dist").fallback(ServeFile::new("web/dist/index.html"))` on the root Axum router `/`.
- If `web/dist` does not exist on disk, server startup skips static file mounting gracefully without failing API tests.

---

## 3. Component Specifications

### 3.1 Authentication & Profiles
- Profile select screen showing available accounts (`admin` default seeded user).
- PIN entry dialog with a 4-digit numeric keypad (supporting keyboard number keys and on-screen clicks).
- Authenticates via `POST /api/v1/auth/pin` (or `/profile-pin`).
- Persists user object and JWT token in `localStorage`.

### 3.2 Media Browser (`/browse`)
- Navigation header: Brand logo, Home, Movies, Shows, Layout Studio, Telemetry, and current User profile avatar.
- Calls `GET /api/v1/screens/:screenId` (`home`, `movies`, `shows`).
- **Widgets:**
  - `SpotlightWidget`: Full-width hero banner with backdrop image, title, synopsis, badges, "Play" button, and "Details" button.
  - `CarouselWidget`: Horizontal scroll rail with poster cards, progress bar indicator for resume items, hover zoom effects.
  - `GridWidget`: Responsive grid layout for catalog browsing.
- **Item Details Modal:**
  - Synopsis, duration, release year, resolution, audio channels.
  - Subtitle management tab: lists existing tracks (`GET /api/v1/items/:id/subtitles`), search online matches (`GET /api/v1/subtitles/:id/search`), and one-click download (`POST /api/v1/subtitles/:id/download`).
  - "Play" button navigating to Cinema Player.

### 3.3 Cinema Player (`/player/:itemId`)
- Fullscreen HTML5 `<video>` element with custom dark overlay controls.
- Fetches stream from `/api/v1/stream/:itemId`.
- Fetches initial resume position from `GET /api/v1/playback/states/:itemId` and seeks automatically.
- Creates session `POST /api/v1/playback/sessions` and reports progress every 10 seconds via `POST /api/v1/playback/:sessionId/progress`.
- Injects `<track>` tags from `GET /api/v1/items/:itemId/subtitles` (`/api/v1/subtitles/:id/stream.vtt`) for native browser subtitle rendering.
- Keyboard bindings:
  - `Space` / `k`: Play / Pause toggle
  - `ArrowLeft` / `ArrowRight`: Seek ±10 seconds
  - `ArrowUp` / `ArrowDown`: Volume ±10%
  - `f`: Fullscreen toggle
  - `m`: Mute toggle
  - `Escape`: Back to browse screen

### 3.4 Layout Studio (`/studio`)
- Screen selector (`Home`, `Movies`, `Shows`).
- Responsive viewport simulator:
  - **TV (16:9)**: 1920x1080 scaled preview
  - **Tablet (4:3)**: 1024x768 preview
  - **Mobile (9:16)**: 390x844 preview
- Widget tree inspector showing node IDs, display types, and query macros with live preview updates.

### 3.5 Telemetry & Live Event Log (`/telemetry`)
- Live resource cards:
  - Process RSS memory (MB)
  - SQLite database file size (MB)
  - SQLite WAL file size (KB / MB)
  - Active streaming sessions count
- Live Event Stream:
  - Connects to `EventSource("/api/v1/events")`.
  - Displays scrolling real-time event log with event badges (`library:updated`, `session:synced`, `subtitle:downloaded`, `system:telemetry`) and timestamps.

---

## 4. Verification & Testing Strategy

1. **Frontend Unit & Component Tests:**
   - Vitest / React Testing Library for PIN keypad input, widget rendering, and player keyboard controls.
2. **Production Asset Build Verification:**
   - `npm run build` generates optimized static bundle in `web/dist`.
3. **Axum Static Serving Integration Test:**
   - Server route test verifying `GET /` serves `index.html` and static JS/CSS assets with proper MIME types.
4. **End-to-End User Journey Test:**
   - Authenticate admin -> query screen layout -> stream video & subtitles -> verify telemetry endpoint.
