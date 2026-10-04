# Kadr UI/UX Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-skin and polish the entire Kadr Web UI according to the official specifications in `theme.md` and `docs/superpowers/specs/2026-10-04-kadr-theme-design.md`, establishing the warm cinema color palette, typography (Inter + JetBrains Mono), 8–12px card geometry, 1px semi-transparent inner elevation borders, and 10-foot TV navigation focus rings.

**Architecture:** Tailwind CSS v4 design tokens via `@theme` in `web/src/index.css`, Google Fonts preloading in `web/index.html`, and systematic re-skinning across the application shell, profile selection, media browser widgets, cinema player HUD, admin dashboard, and telemetry screens.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4 (`@tailwindcss/vite`), Vite 6, Lucide React, Vitest.

## Global Constraints
- Strictly match the color hex codes from `theme.md`:
  - Canvas: `#2A0A0A`
  - Panel: `#3B1111` (hover `#4A1616`)
  - Accent: `#FF6B00`
  - Highlight: `#FFB800`
  - CTA: `#D43F15` (hover `#B83410`)
  - Text Primary: `#F5E5E5`
  - Muted: `#5C3C3C`
  - Border: `rgba(245, 229, 229, 0.08)`
- 8px to 12px border radius (`rounded-lg` / `rounded-xl`) on all primary UI cards, dialogs, inputs, and buttons.
- 10-foot TV UI focus ring: `3px` solid `#FFB800` (`focus-visible:ring-3 focus-visible:ring-highlight`).
- All 77 frontend tests (`npm test -- --run`) and production build (`npm run build`) must pass 100% cleanly.
- All workspace Rust tests (`cargo test --workspace`) and clippy checks must remain 100% passing.

---

### Task 1: Theme Tokens, Typography Preloading & Global Styles (`web/src/index.css` & `web/index.html`)

**Files:**
- Modify: `web/src/index.css:1-51`
- Modify: `web/index.html:1-14`
- Test: `web/src/App.test.tsx`

**Interfaces:**
- Produces: Tailwind v4 theme utility classes (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `bg-cta`, `text-text-main`, `text-muted`, `border-border-subtle`, `font-sans`, `font-mono`)

- [ ] **Step 1: Update index.css with @theme tokens**
Edit `web/src/index.css`:
```css
@import "tailwindcss";

@theme {
  --color-canvas: #2A0A0A;
  --color-panel: #3B1111;
  --color-panel-hover: #4A1616;
  --color-accent: #FF6B00;
  --color-highlight: #FFB800;
  --color-cta: #D43F15;
  --color-cta-hover: #B83410;
  --color-text-main: #F5E5E5;
  --color-muted: #5C3C3C;
  --color-border-subtle: rgba(245, 229, 229, 0.08);

  --font-sans: 'Inter', ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  --font-mono: 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}

@layer base {
  :root {
    color-scheme: dark;
  }

  body {
    background-color: var(--color-canvas);
    color: var(--color-text-main);
    font-family: var(--font-sans);
    overflow-x: hidden;
  }
}

/* Custom cinema scrollbars */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: #2A0A0A;
}

::-webkit-scrollbar-thumb {
  background: #5C3C3C;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #FF6B00;
}

/* Cinema keypad shake animation */
@keyframes shake {
  0%, 100% {
    transform: translateX(0);
  }
  20%, 60% {
    transform: translateX(-8px);
  }
  40%, 80% {
    transform: translateX(8px);
  }
}

.animate-shake {
  animation: shake 0.5s cubic-bezier(0.36, 0.07, 0.19, 0.97) both;
}
```

- [ ] **Step 2: Preload Google Fonts in index.html**
Update `web/index.html`:
```html
<!doctype html>
<html lang="en" class="dark">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/png" href="/kadr-logo-ui.png" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Kadr - Cinema Media Server</title>
    <!-- Preconnect Google Fonts for Inter and JetBrains Mono -->
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
  </head>
  <body class="bg-canvas text-text-main font-sans antialiased selection:bg-accent selection:text-white min-h-screen">
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 3: Run tests and build to verify clean compilation**
Run: `cd web && npm test -- --run && npm run build`
Expected: 100% PASS

- [ ] **Step 4: Commit**
```bash
git add web/src/index.css web/index.html
git commit -m "feat(web): configure theme.md design tokens in Tailwind v4 and load Inter/JetBrains Mono fonts"
```

---

### Task 2: App Shell, Navigation Header & Footer Re-Skinning (`web/src/App.tsx`)

**Files:**
- Modify: `web/src/App.tsx:80-250`
- Test: `web/src/App.test.tsx`

**Interfaces:**
- Header uses `bg-panel/95 backdrop-blur-md border-b border-border-subtle`
- Navigation tabs use `text-muted hover:text-text-main` (inactive) and `text-accent font-semibold bg-canvas/70 border border-border-subtle` (active)
- 10-foot focus ring `focus-visible:ring-3 focus-visible:ring-highlight`

- [ ] **Step 1: Re-skin App.tsx layout, header, navigation, and user menu**
Update `web/src/App.tsx`:
- Root container: `min-h-screen bg-canvas text-text-main flex flex-col font-sans selection:bg-accent selection:text-white`.
- Header: `sticky top-0 z-40 w-full border-b border-border-subtle bg-panel/95 backdrop-blur-md`.
- Logo & Brand: `text-xl font-bold tracking-tight text-text-main group-hover:text-accent transition-colors`.
- Navigation tabs:
  - Inactive: `text-muted hover:text-text-main hover:bg-panel-hover rounded-xl transition-colors`.
  - Active: `text-accent bg-canvas/70 font-semibold border border-border-subtle rounded-xl`.
  - Focus: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.
- Private library badges: locked in `text-highlight`, unlocked in `text-accent`.
- User menu button & profile popover: `bg-panel border border-border-subtle rounded-xl`.

- [ ] **Step 2: Run App component tests**
Run: `cd web && npm test -- --run App.test.tsx`
Expected: 100% PASS

- [ ] **Step 3: Commit**
```bash
git add web/src/App.tsx
git commit -m "feat(web): re-skin app shell, navigation header, and focus states matching theme.md"
```

---

### Task 3: Authentication & Profile Selection Re-Skinning (`ProfileSelect.tsx` & `PinKeypad.tsx`)

**Files:**
- Modify: `web/src/components/auth/ProfileSelect.tsx:1-170`
- Modify: `web/src/components/auth/PinKeypad.tsx:1-160`
- Test: `web/src/components/auth/ProfileSelect.test.tsx`
- Test: `web/src/components/auth/PinKeypad.test.tsx`

**Interfaces:**
- `ProfileSelect` renders `bg-canvas text-text-main`, cards in `bg-panel hover:bg-panel-hover border border-border-subtle rounded-xl`
- `PinKeypad` renders PIN dots (`#FFB800` filled, `#5C3C3C` empty) and keypad buttons in `bg-canvas/60 hover:bg-canvas border border-border-subtle rounded-xl`

- [ ] **Step 1: Re-skin ProfileSelect.tsx**
Update `ProfileSelect.tsx`:
- Main wrapper: `w-full max-w-4xl mx-auto flex flex-col items-center justify-center py-8 px-4`.
- Header: Title in `text-text-main`, description in `text-muted`.
- Profile cards: `bg-panel hover:bg-panel-hover border border-border-subtle rounded-xl p-1 transition-all duration-200 focus-visible:ring-3 focus-visible:ring-highlight`.
- Role badges: Admin in `bg-cta text-white`, standard in `bg-panel text-muted border border-border-subtle`.
- Switch profile / Sign out buttons: `text-muted hover:text-text-main`.

- [ ] **Step 2: Re-skin PinKeypad.tsx**
Update `PinKeypad.tsx`:
- Container: `bg-panel border border-border-subtle rounded-2xl p-6 sm:p-8 max-w-sm w-full mx-auto`.
- PIN dots:
  - Filled: `bg-highlight scale-110 shadow-md shadow-highlight/30`.
  - Empty: `bg-muted/40 border border-muted`.
- Digits grid (0-9): `bg-canvas/60 hover:bg-canvas text-text-main hover:text-white border border-border-subtle rounded-xl text-2xl font-bold h-14 w-14 transition-all duration-150 active:scale-95 focus-visible:ring-3 focus-visible:ring-highlight`.
- Action buttons: Backspace and Cancel in `text-muted hover:text-text-main hover:bg-canvas/40 rounded-xl`.

- [ ] **Step 3: Run auth component tests**
Run: `cd web && npm test -- --run ProfileSelect.test.tsx PinKeypad.test.tsx`
Expected: 100% PASS

- [ ] **Step 4: Commit**
```bash
git add web/src/components/auth/
git commit -m "feat(web): re-skin profile selection and PIN keypad matching theme.md color system"
```

---

### Task 4: Media Browser & Widget Components Re-Skinning (`BrowseScreen`, `SpotlightWidget`, `CarouselWidget`, `GridWidget`, `ItemDetailsModal`)

**Files:**
- Modify: `web/src/components/browse/SpotlightWidget.tsx:1-250`
- Modify: `web/src/components/browse/CarouselWidget.tsx:1-200`
- Modify: `web/src/components/browse/GridWidget.tsx:1-200`
- Modify: `web/src/components/browse/ItemDetailsModal.tsx:1-350`
- Modify: `web/src/components/browse/BrowseScreen.tsx:1-180`
- Test: `web/src/components/browse/BrowseScreen.test.tsx`

**Interfaces:**
- Spotlight Hero: Play CTA button `bg-cta hover:bg-cta-hover text-white rounded-xl shadow-lg shadow-cta/20`
- Media cards: `rounded-xl border border-border-subtle`, hover reveals `#D43F15` action badge, metadata `text-muted truncate`
- 10-foot focus ring: `focus-visible:ring-3 focus-visible:ring-highlight`
- Item Details Modal: `bg-panel border border-border-subtle rounded-2xl`

- [ ] **Step 1: Re-skin SpotlightWidget.tsx**
Update `SpotlightWidget.tsx`:
- Backdrop gradient: Dark fade to `#2A0A0A` (`from-canvas via-canvas/60 to-transparent`).
- Action buttons:
  - Play button: `bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl shadow-lg shadow-cta/25 focus-visible:ring-3 focus-visible:ring-highlight`.
  - More Info button: `bg-panel/80 hover:bg-panel text-text-main border border-border-subtle rounded-xl`.
- Badges: Rating badge in `bg-highlight/20 text-highlight border border-highlight/40`.
- Text: Title in `text-text-main`, synopsis in `text-muted`.

- [ ] **Step 2: Re-skin CarouselWidget.tsx & GridWidget.tsx**
Update `CarouselWidget.tsx` and `GridWidget.tsx`:
- Section headers: Title in `text-text-main`, count in `text-muted`.
- Poster cards:
  - Container: `rounded-xl overflow-hidden border border-border-subtle bg-panel hover:bg-panel-hover transition-all duration-200 focus-visible:ring-3 focus-visible:ring-highlight`.
  - Hover action overlay: Lightens card and reveals `#D43F15` Accent Red play icon overlay.
  - Resume progress bar: `bg-muted` track, `bg-accent` progress fill.
  - Card text: Primary title in `text-text-main font-medium truncate`, metadata (year, runtime) in `text-muted text-xs truncate`.
- Carousel pagination arrows: `bg-panel/90 hover:bg-panel text-text-main border border-border-subtle rounded-full`.

- [ ] **Step 3: Re-skin ItemDetailsModal.tsx**
Update `ItemDetailsModal.tsx`:
- Modal container: `bg-panel border border-border-subtle rounded-2xl max-w-4xl w-full shadow-2xl`.
- Technical readouts (codecs, bitrate, resolution, container): `font-mono text-xs text-text-main bg-canvas/60 px-2.5 py-1 rounded-lg border border-border-subtle`.
- Play button: `bg-cta hover:bg-cta-hover text-white rounded-xl shadow-lg shadow-cta/25`.
- Subtitle tracks & download buttons: `bg-canvas/50 border border-border-subtle rounded-xl hover:bg-canvas`, download CTA in `bg-cta hover:bg-cta-hover text-white rounded-lg`.

- [ ] **Step 4: Run browse screen component tests**
Run: `cd web && npm test -- --run BrowseScreen.test.tsx`
Expected: 100% PASS

- [ ] **Step 5: Commit**
```bash
git add web/src/components/browse/
git commit -m "feat(web): re-skin media widgets, cards, and item details modal matching theme.md"
```

---

### Task 5: Cinema Player HUD & Controls Re-Skinning (`CinemaPlayer.tsx` & `PlayerControls.tsx`)

**Files:**
- Modify: `web/src/components/player/PlayerControls.tsx:1-350`
- Modify: `web/src/components/player/CinemaPlayer.tsx:1-300`
- Test: `web/src/components/player/CinemaPlayer.test.tsx`

**Interfaces:**
- Scrobble progress bar: Active fill in `#FF6B00` (Radiant Accent)
- Volume slider & highlight indicators: Golden Highlight `#FFB800`
- HUD containers: `bg-canvas/90 backdrop-blur-md border border-border-subtle rounded-xl`
- Icon buttons: Outlined `#5C3C3C` transitioning to `#FF6B00` on hover or active

- [ ] **Step 1: Re-skin PlayerControls.tsx**
Update `PlayerControls.tsx`:
- Controls overlay: `bg-gradient-to-t from-canvas via-canvas/80 to-transparent p-6`.
- Progress bar:
  - Track: `bg-muted/40 h-1.5 hover:h-2.5 rounded-full transition-all cursor-pointer`.
  - Progress fill: `bg-accent rounded-full relative`.
  - Scrobble thumb: `bg-highlight w-3.5 h-3.5 rounded-full shadow-md`.
- Time display: `font-mono text-xs text-text-main`.
- Control icons (Play, Pause, Skip, Subtitles, Volume):
  - Inactive: `text-muted hover:text-accent transition-colors`.
  - Active toggle (e.g. Subtitles ON): `text-accent`.
- Volume slider: Active fill in `bg-highlight`, thumb in `bg-highlight`.
- Subtitle selection menu: `bg-panel border border-border-subtle rounded-xl p-2 shadow-2xl`.

- [ ] **Step 2: Re-skin CinemaPlayer.tsx**
Update `CinemaPlayer.tsx`:
- Video player canvas: `bg-canvas`.
- Loading spinner: `border-accent border-t-transparent`.
- Resume banner / prompt: `bg-panel/95 border border-border-subtle rounded-xl text-text-main`.

- [ ] **Step 3: Run player component tests**
Run: `cd web && npm test -- --run CinemaPlayer.test.tsx`
Expected: 100% PASS

- [ ] **Step 4: Commit**
```bash
git add web/src/components/player/
git commit -m "feat(web): re-skin cinema player HUD, progress bar, and volume controls matching theme.md"
```

---

### Task 6: Admin Dashboard, Layout Studio & System Telemetry Re-Skinning (`AdminDashboard.tsx`, `LayoutStudio.tsx`, `TelemetryDashboard.tsx`)

**Files:**
- Modify: `web/src/components/admin/AdminDashboard.tsx:1-700`
- Modify: `web/src/components/studio/LayoutStudio.tsx:1-350`
- Modify: `web/src/components/telemetry/TelemetryDashboard.tsx:1-350`
- Test: `web/src/components/admin/AdminDashboard.test.tsx`
- Test: `web/src/components/studio/LayoutStudio.test.tsx`
- Test: `web/src/components/telemetry/TelemetryDashboard.test.tsx`

**Interfaces:**
- Admin cards, studio panels, and telemetry cards: `bg-panel border border-border-subtle rounded-xl`
- CTA buttons ("Add Library", "Save Configuration"): `bg-cta hover:bg-cta-hover text-white rounded-lg shadow-md`
- Private badges: `bg-highlight/20 text-highlight border border-highlight/40`
- Technical telemetry readouts: `font-mono text-text-main`

- [ ] **Step 1: Re-skin AdminDashboard.tsx**
Update `AdminDashboard.tsx`:
- Page header: `text-3xl font-bold tracking-tight text-text-main`, description in `text-muted`.
- Tabs: Inactive in `text-muted hover:text-text-main hover:bg-panel-hover rounded-xl`, active in `bg-accent text-canvas font-semibold rounded-xl shadow-md`.
- Library cards: `bg-panel border border-border-subtle rounded-xl p-5 hover:bg-panel-hover transition-colors`.
- Private badge: `bg-highlight/15 text-highlight border border-highlight/30 rounded-full text-xs font-semibold`.
- Primary CTA buttons ("Add Library", "Add User Profile", "Save Configuration"): `bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl shadow-md`.
- Modals & dialogs: `bg-panel border border-border-subtle rounded-2xl shadow-2xl`.

- [ ] **Step 2: Re-skin LayoutStudio.tsx**
Update `LayoutStudio.tsx`:
- Screen selector tabs: Inactive `text-muted hover:text-text-main`, active `bg-panel text-accent font-semibold border border-border-subtle rounded-xl`.
- Viewport frame simulator: Device bezels in `bg-panel border border-border-subtle rounded-2xl shadow-2xl`.
- AST widget tree inspector: Monospace JSON node readouts in `font-mono text-xs text-text-main bg-canvas/80 p-3 rounded-xl border border-border-subtle`.

- [ ] **Step 3: Re-skin TelemetryDashboard.tsx**
Update `TelemetryDashboard.tsx`:
- Metric cards (Memory RSS, DB Size, WAL Size, Active Sessions): `bg-panel border border-border-subtle rounded-xl p-5`.
- Numbers & readouts: `font-mono text-2xl font-bold text-text-main`.
- Live SSE event feed: Table/log container in `bg-panel border border-border-subtle rounded-xl`, timestamps in `font-mono text-xs text-muted`.

- [ ] **Step 4: Run all remaining component tests**
Run: `cd web && npm test -- --run AdminDashboard.test.tsx LayoutStudio.test.tsx TelemetryDashboard.test.tsx`
Expected: 100% PASS

- [ ] **Step 5: Run full frontend test suite and build**
Run: `cd web && npm test -- --run`
Run: `cd web && npm run build`
Expected: 100% PASS (77/77 tests), clean build in `web/dist`

- [ ] **Step 6: Run full workspace Rust tests and clippy**
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 100% PASS and 0 warnings

- [ ] **Step 7: Commit**
```bash
git add web/src/components/admin/ web/src/components/studio/ web/src/components/telemetry/
git commit -m "feat(web): re-skin admin dashboard, layout studio, and telemetry dashboard matching theme.md"
```
