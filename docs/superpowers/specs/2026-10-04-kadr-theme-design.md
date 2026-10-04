# Design Specification: Kadr UI/UX Theme Implementation

**Document Version:** 1.0.0  
**Date:** 2026-10-04  
**Status:** Approved  
**Source Specification:** `theme.md`  

---

## 1. Overview & Objectives

This specification outlines the full re-skinning and UI refinement of the Kadr Web Application to match the official design language documented in `theme.md`.

### Core Requirements
1. **Tailwind v4 Semantic Theme Tokens**:
   - Primary Background: `#2A0A0A`
   - Secondary Background: `#3B1111` (hover `#4A1616`)
   - Radiant Accent: `#FF6B00`
   - Golden Highlight: `#FFB800`
   - Accent Red (CTA): `#D43F15` (hover `#B83410`)
   - Text Primary: `#F5E5E5`
   - Muted Elements: `#5C3C3C`
   - Elevation Border: `rgba(245, 229, 229, 0.08)`
2. **Typography**:
   - Preload Google Fonts: **Inter** (Primary Interface, weights 400, 500, 600, 700) and **JetBrains Mono** (Technical Readouts, Bitrate, Paths, Telemetry).
   - Strict ellipsis truncation on long metadata strings (actors, genres, titles) to preserve grid layouts.
3. **Geometry & Elevation**:
   - `8px` to `12px` border radius across cards, buttons, dialogs, and inputs.
   - 1px semi-transparent inner border (`rgba(245, 229, 229, 0.08)`) replacing harsh drop shadows.
   - High-contrast `3px` solid `#FFB800` (Golden Highlight) focus rings for 10-foot UI / TV remote navigation.
4. **Interactive States & Iconography**:
   - Inactive menu/control items in stroked `#5C3C3C`.
   - Active/selected items in filled or vibrant `#FF6B00`.
   - Hover states lighten container backgrounds by 5–10% and subtly reveal `#D43F15` Accent Red.

---

## 2. Token Architecture (`web/src/index.css`)

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
```

---

## 3. Component Re-Skinning Specifications

### 3.1 App Shell & Navigation Header (`web/src/App.tsx`)
- Header: `bg-panel/95 backdrop-blur-md border-b border-border-subtle`.
- Kadr Logo: Render `kadr-logo-ui.png` with wordmark `text-text-main hover:text-accent`.
- Navigation links:
  - Inactive: `text-muted hover:text-text-main hover:bg-panel-hover`.
  - Active: `text-accent font-semibold bg-canvas/70 border border-border-subtle`.
- 10-foot focus rings: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.

### 3.2 Authentication & Profile Selection (`ProfileSelect.tsx` & `PinKeypad.tsx`)
- Page container: `bg-canvas text-text-main`.
- Profile Avatar Cards: `bg-panel hover:bg-panel-hover border border-border-subtle rounded-xl`.
- PIN Keypad:
  - Dialog container: `bg-panel border border-border-subtle rounded-2xl`.
  - PIN Dots: Filled `#FFB800` (Golden Highlight), empty `#5C3C3C`.
  - Numeric Digits: `bg-canvas/60 hover:bg-canvas text-text-main border border-border-subtle rounded-xl`.

### 3.3 Media Browser & Widgets (`BrowseScreen.tsx`, `SpotlightWidget.tsx`, `CarouselWidget.tsx`, `GridWidget.tsx`, `ItemDetailsModal.tsx`)
- Spotlight Hero:
  - Primary Play button: `bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl shadow-lg shadow-cta/20`.
  - More Info button: `bg-panel/80 hover:bg-panel text-text-main border border-border-subtle rounded-xl`.
- Media Cards:
  - `rounded-xl` (12px) border radius with `border border-border-subtle`.
  - Hover action overlay reveals `#D43F15` play badge.
  - Secondary metadata (year, runtime, genre): `text-muted truncate`.
  - TV / Gamepad focus state: `focus-visible:ring-3 focus-visible:ring-highlight`.

### 3.4 Cinema Player HUD (`CinemaPlayer.tsx` & `PlayerControls.tsx`)
- Controls overlay: `bg-canvas/90 backdrop-blur-md border border-border-subtle rounded-xl`.
- Scrobbler progress bar: Filled with `#FF6B00` (Radiant Accent).
- Volume slider & highlight badges: Styled with `#FFB800` (Golden Highlight).
- Icon buttons: Outlined `#5C3C3C` transitioning to filled/vibrant `#FF6B00` on hover or active.

### 3.5 Admin Dashboard & Telemetry (`AdminDashboard.tsx`, `TelemetryDashboard.tsx`)
- Dashboard cards & modals: `bg-panel border border-border-subtle rounded-xl`.
- Primary CTA buttons ("Add Library", "Save Configuration"): `bg-cta hover:bg-cta-hover text-white rounded-lg`.
- Technical readouts (bitrate, memory RSS, DB sizes, connection logs): `font-mono text-text-main`.

---

## 4. Verification & Testing

1. All 77 frontend tests must pass (`cd web && npm test -- --run`).
2. Production build must compile cleanly (`cd web && npm run build`).
3. All workspace Rust tests (`cargo test --workspace`) and clippy checks (`cargo clippy --workspace --all-targets -- -D warnings`) must remain 100% passing.
