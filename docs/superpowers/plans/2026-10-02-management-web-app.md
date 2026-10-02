# Milestone 5B: Management Web App & Layout Studio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete, responsive Single-Page Application (SPA) for Kadr in `web/` using Vite + React + TypeScript + Tailwind CSS, featuring PIN authentication, declarative AST media browsing, full-screen cinema video player with native WebVTT subtitles and playback scrobbling, interactive layout studio, and real-time telemetry dashboard, served directly by Axum.

**Architecture:** Client-side SPA built to `web/dist` and served statically by the Axum server using `tower-http::services::ServeDir` with fallback to `index.html`. Typed API client connecting to Kadr's REST endpoints (`/auth`, `/screens`, `/widgets`, `/items`, `/stream`, `/subtitles`, `/system/telemetry`) and EventSource connecting to `/api/v1/events`.

**Tech Stack:** React 19, TypeScript, Vite 6, Tailwind CSS, Lucide React, Vitest / React Testing Library, Axum 0.8, Tower-HTTP 0.6.

## Global Constraints

- Zero external C/native runtime dependencies baseline on the Rust server (musl target compatibility).
- Axum static file serving must fall back gracefully if `web/dist` does not exist (e.g. during headless Rust tests).
- All authenticated API requests from the frontend must include `Authorization: Bearer <token>` from `localStorage`.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and clippy checks (`cargo clippy --workspace --all-targets -- -D warnings`) must remain 100% passing.

---

### Task 1: Web App Project Scaffolding, Typed API Client & Shell (`web/`)

**Files:**
- Create: `web/package.json`
- Create: `web/vite.config.ts`
- Create: `web/tsconfig.json`
- Create: `web/src/main.tsx`
- Create: `web/src/index.css`
- Create: `web/src/App.tsx`
- Create: `web/src/api/client.ts`
- Create: `web/src/types/index.ts`
- Test: `web/src/api/client.test.ts`

**Interfaces:**
- Consumes: Backend REST API contracts (`/api/v1/*`)
- Produces:
  - Working Vite project compiling with `npm run build`
  - Typed API client (`getScreens`, `getScreen`, `getWidgetData`, `getItemDetails`, `loginWithPin`, `getSubtitles`, `getTelemetry`, etc.)
  - Auth context and navigation shell

- [ ] **Step 1: Write test for API client**
Create `web/src/api/client.test.ts` testing auth header injection, error handling, and response deserialization.

- [ ] **Step 2: Initialize Vite + React + Tailwind project**
Create `web/package.json` with dependencies (React 19, Lucide React, Tailwind CSS, Vite, Vitest), `vite.config.ts` with API proxy to `http://localhost:8080`, and TypeScript configs.

- [ ] **Step 3: Run npm install**
Run: `cd web && npm install`

- [ ] **Step 4: Implement API client and types**
Implement `web/src/types/index.ts` (mirroring `CardViewModel`, `ScreenLayout`, `WidgetNode`, `TelemetrySnapshot`, `SystemEvent`), and `web/src/api/client.ts`.

- [ ] **Step 5: Run tests and build**
Run: `cd web && npm test -- --run && npm run build`
Expected: PASS

- [ ] **Step 6: Commit**
Commit: `feat(web): scaffold Vite React app with typed API client and shell`

---

### Task 2: Profile Selection & 4-Digit PIN Keypad Authentication (`web/`)

**Files:**
- Create: `web/src/components/auth/ProfileSelect.tsx`
- Create: `web/src/components/auth/PinKeypad.tsx`
- Modify: `web/src/App.tsx`
- Test: `web/src/components/auth/PinKeypad.test.tsx`

**Interfaces:**
- Consumes: `POST /api/v1/auth/pin` via API client
- Produces:
  - Interactive profile avatar list with `admin` account
  - 4-digit numeric keypad supporting physical keyboard and on-screen clicks
  - Auto-submission on 4th digit, shake animation on invalid PIN
  - Persisting JWT and user state in `localStorage`

- [ ] **Step 1: Write failing component test**
Create `web/src/components/auth/PinKeypad.test.tsx` testing digit accumulation, backspace, and callback execution on complete 4-digit entry.

- [ ] **Step 2: Run test to verify it fails**
Run: `cd web && npm test -- --run PinKeypad.test.tsx`
Expected: FAIL

- [ ] **Step 3: Implement PinKeypad and ProfileSelect**
Implement `PinKeypad.tsx` and `ProfileSelect.tsx` with dark cinema aesthetic, smooth animations, and token persistence.

- [ ] **Step 4: Run test to verify it passes**
Run: `cd web && npm test -- --run PinKeypad.test.tsx`
Expected: PASS

- [ ] **Step 5: Run web build and workspace verification**
Run: `cd web && npm run build && cd .. && cargo test --workspace`
Expected: PASS

- [ ] **Step 6: Commit**
Commit: `feat(web): implement profile selection and 4-digit PIN keypad authentication`

---

### Task 3: Declarative Media Browser & Item Details Modal (`web/`)

**Files:**
- Create: `web/src/components/browse/BrowseScreen.tsx`
- Create: `web/src/components/browse/SpotlightWidget.tsx`
- Create: `web/src/components/browse/CarouselWidget.tsx`
- Create: `web/src/components/browse/GridWidget.tsx`
- Create: `web/src/components/browse/ItemDetailsModal.tsx`
- Modify: `web/src/App.tsx`
- Test: `web/src/components/browse/BrowseScreen.test.tsx`

**Interfaces:**
- Consumes:
  - `GET /api/v1/screens/:screen_id`
  - `GET /api/v1/widgets/:widget_id/data`
  - `GET /api/v1/items/:item_id/details`
  - `GET /api/v1/artwork/:item_id/poster`, `/backdrop`
  - Subtitle management endpoints (`/subtitles`)
- Produces:
  - Responsive media browsing interface for Home, Movies, and Shows
  - Spotlight hero banner with backdrop and action buttons
  - Horizontal scrolling carousels with hover expansion and progress bars
  - Rich item details modal with metadata inspection and subtitle search/download

- [ ] **Step 1: Write failing test for BrowseScreen**
Create `web/src/components/browse/BrowseScreen.test.tsx` testing widget rendering, spotlight hero display, and details modal opening.

- [ ] **Step 2: Run test to verify it fails**
Run: `cd web && npm test -- --run BrowseScreen.test.tsx`
Expected: FAIL

- [ ] **Step 3: Implement browse widgets and details modal**
Implement `BrowseScreen.tsx`, `SpotlightWidget.tsx`, `CarouselWidget.tsx`, `GridWidget.tsx`, and `ItemDetailsModal.tsx`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cd web && npm test -- --run BrowseScreen.test.tsx`
Expected: PASS

- [ ] **Step 5: Verify build**
Run: `cd web && npm run build`
Expected: PASS with clean bundle output.

- [ ] **Step 6: Commit**
Commit: `feat(web): implement declarative media browser and item details modal`

---

### Task 4: Cinema Player with Native Subtitles & Scrobbling (`web/`)

**Files:**
- Create: `web/src/components/player/CinemaPlayer.tsx`
- Create: `web/src/components/player/PlayerControls.tsx`
- Modify: `web/src/App.tsx`
- Test: `web/src/components/player/CinemaPlayer.test.tsx`

**Interfaces:**
- Consumes:
  - `GET /api/v1/stream/:item_id` (HTTP 206 video stream)
  - `GET /api/v1/playback/states/:item_id` (initial resume position)
  - `POST /api/v1/playback/sessions` (session start)
  - `POST /api/v1/playback/:session_id/progress` (10s heartbeat scrobble)
  - `GET /api/v1/items/:item_id/subtitles` (subtitle track list)
  - `GET /api/v1/subtitles/:subtitle_id/stream.vtt` (native WebVTT stream)
- Produces:
  - Cinema-grade HTML5 video player with custom auto-hiding controls
  - Native `<track>` rendering and on-screen audio/subtitle menu
  - Automatic resume from last watched timestamp
  - Responsive keyboard shortcuts (Space, Arrow keys, F, M, Esc)

- [ ] **Step 1: Write failing test for CinemaPlayer**
Create `web/src/components/player/CinemaPlayer.test.tsx` testing subtitle track injection, keyboard shortcuts, and progress heartbeat triggering.

- [ ] **Step 2: Run test to verify it fails**
Run: `cd web && npm test -- --run CinemaPlayer.test.tsx`
Expected: FAIL

- [ ] **Step 3: Implement CinemaPlayer and PlayerControls**
Implement `CinemaPlayer.tsx` and `PlayerControls.tsx` with resume handling, periodic heartbeats, and native subtitle track toggles.

- [ ] **Step 4: Run test to verify it passes**
Run: `cd web && npm test -- --run CinemaPlayer.test.tsx`
Expected: PASS

- [ ] **Step 5: Verify build**
Run: `cd web && npm run build`
Expected: PASS

- [ ] **Step 6: Commit**
Commit: `feat(web): implement cinema video player with native WebVTT subtitles and scrobbling`

---

### Task 5: Layout Studio & System Telemetry Dashboard (`web/`)

**Files:**
- Create: `web/src/components/studio/LayoutStudio.tsx`
- Create: `web/src/components/telemetry/TelemetryDashboard.tsx`
- Modify: `web/src/App.tsx`
- Test: `web/src/components/studio/LayoutStudio.test.tsx`
- Test: `web/src/components/telemetry/TelemetryDashboard.test.tsx`

**Interfaces:**
- Consumes:
  - `GET /api/v1/screens` and screen ASTs
  - `GET /api/v1/system/telemetry` (snapshot metrics)
  - `GET /api/v1/events` (real-time SSE stream)
- Produces:
  - Layout Studio with TV (16:9), Tablet (4:3), and Mobile (9:16) viewport frame simulators
  - System Telemetry Dashboard with metric cards (RSS, DB/WAL sizes, active sessions) and real-time live event feed

- [ ] **Step 1: Write failing tests**
Create `LayoutStudio.test.tsx` and `TelemetryDashboard.test.tsx`.

- [ ] **Step 2: Run tests to verify they fail**
Run: `cd web && npm test -- --run LayoutStudio.test.tsx TelemetryDashboard.test.tsx`
Expected: FAIL

- [ ] **Step 3: Implement LayoutStudio and TelemetryDashboard**
Implement `LayoutStudio.tsx` and `TelemetryDashboard.tsx` connecting to EventSource `/api/v1/events`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cd web && npm test -- --run LayoutStudio.test.tsx TelemetryDashboard.test.tsx`
Expected: PASS

- [ ] **Step 5: Verify build**
Run: `cd web && npm run build`
Expected: PASS

- [ ] **Step 6: Commit**
Commit: `feat(web): implement layout studio preview and system telemetry dashboard`

---

### Task 6: Axum Static File Serving & Full Milestone 5 Integration Test (`kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/api/mod.rs`
- Modify: `crates/kadr-server/src/main.rs`
- Modify: `crates/kadr-server/Cargo.toml`
- Create: `tests/e2e_web_serving_test.rs`

**Interfaces:**
- Consumes: `web/dist` production assets
- Produces:
  - Axum server serving compiled static assets from `web/dist` with SPA fallback to `index.html`
  - Integration test verifying `GET /`, static asset streaming, and API route preservation
  - `[[test]]` entry in `crates/kadr-server/Cargo.toml`

- [ ] **Step 1: Write failing integration test**
Create `tests/e2e_web_serving_test.rs` testing:
- Request to `GET /` returns 200 OK with `text/html` containing the Kadr SPA root.
- Request to static JS/CSS returns 200 with proper MIME types.
- Request to an unknown client route (e.g. `/browse`) falls back to `index.html`.
- API routes (`/api/v1/*`) remain intact and unaffected.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test e2e_web_serving_test`
Expected: FAIL

- [ ] **Step 3: Mount ServeDir with SPA fallback in Axum router**
Update `crates/kadr-server/src/api/mod.rs` to mount static serving for `web/dist` when the directory exists.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test e2e_web_serving_test`
Expected: PASS

- [ ] **Step 5: Run full workspace test suite and clippy**
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: All tests pass, 0 clippy warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): mount static SPA file serving for web app and add Milestone 5 E2E test`
