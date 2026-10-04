# Streamlined Top Navigation & Unified Admin Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Streamline the top navigation in `App.tsx` to display only the user's actual libraries (plus Home and Admin), removing hardcoded Movies/Shows and developer tools, while embedding Layout Studio and System Telemetry into `AdminDashboard.tsx`.

**Architecture:**
* `AdminDashboard.tsx`: Expand admin tabs to `'libraries' | 'users' | 'studio' | 'telemetry' | 'config'`, embedding `LayoutStudio` and `TelemetryDashboard` under dedicated administrative tabs.
* `App.tsx`: Remove hardcoded `"Movies"`, `"Shows"`, `"Studio"`, and `"Telemetry"` buttons from the header navigation. Dynamically resolve icons for user libraries based on `lib.media_type` (`Film`, `Tv`, `Sparkles`, `Music`, `Video`, `BookOpen`), and pass `onPlayItem` down to `AdminDashboard`.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4, Lucide React, Vitest.

## Global Constraints
* Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `bg-cta`, `text-text-main`, `text-muted`, `border-border-subtle`).
* 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.
* Accessible navigation and tab semantics (`role="tablist"`, `role="tab"` or button controls).
* Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
* 100% test pass rate across Rust workspace (`cargo test --workspace`) and frontend tests (`npm test -- --run`).

---

### Task 1: AdminDashboard Tab Expansion (Embed Layout Studio & System Telemetry)

**Files:**
* Modify: `web/src/components/admin/AdminDashboard.tsx:23-95`
* Modify: `web/src/components/admin/AdminDashboard.test.tsx`

**Interfaces:**
* Produces:
  * `AdminTab`: `'libraries' | 'users' | 'studio' | 'telemetry' | 'config'`.
  * `AdminDashboardProps`: `{ onClose?: () => void; onPlayItem?: (itemId: number) => void; initialTab?: AdminTab; }`.
  * Renders `<LayoutStudio onPlayItem={onPlayItem} />` when `activeTab === 'studio'`.
  * Renders `<TelemetryDashboard />` when `activeTab === 'telemetry'`.

- [ ] **Step 1: Write failing tests in `AdminDashboard.test.tsx`**

Add tests verifying:
1. All 5 tabs ("Libraries", "Users", "Layout Studio", "Telemetry", "Configuration") are rendered in the admin header.
2. Clicking "Layout Studio" tab switches view and renders Layout Studio content.
3. Clicking "Telemetry" tab switches view and renders Telemetry content.

```typescript
it('renders all 5 admin tabs and switches to Layout Studio and Telemetry', async () => {
  render(<AdminDashboard />);
  await waitFor(() => expect(screen.getByRole('button', { name: /Libraries/i })).toBeInTheDocument());
  expect(screen.getByRole('button', { name: /Users/i })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Layout Studio/i })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Telemetry/i })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Configuration/i })).toBeInTheDocument();

  // Switch to Layout Studio
  fireEvent.click(screen.getByRole('button', { name: /Layout Studio/i }));
  expect(await screen.findByText(/Declarative AST screen inspector/i)).toBeInTheDocument();

  // Switch to Telemetry
  fireEvent.click(screen.getByRole('button', { name: /Telemetry/i }));
  expect(await screen.findByText(/System Telemetry/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd web && npm test -- --run AdminDashboard.test.tsx`
Expected: FAIL (missing Layout Studio and Telemetry tab buttons).

- [ ] **Step 3: Implement tab expansion in `AdminDashboard.tsx`**

In `web/src/components/admin/AdminDashboard.tsx`:
1. Import `LayoutStudio` from `../studio/LayoutStudio` and `TelemetryDashboard` from `../telemetry/TelemetryDashboard`.
2. Import `Layout` and `Activity` icons from `lucide-react`.
3. Update `AdminDashboardProps` to accept `onPlayItem?: (itemId: number) => void; initialTab?: AdminTab;`.
4. Update `activeTab` state type to `'libraries' | 'users' | 'studio' | 'telemetry' | 'config'`.
5. Add button tabs for "Layout Studio" and "Telemetry" with proper styling and focus rings.
6. Render `<LayoutStudio onPlayItem={onPlayItem} />` and `<TelemetryDashboard />` when their respective tabs are selected.

- [ ] **Step 4: Run tests and verify passing**

Run: `cd web && npm test -- --run AdminDashboard.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/admin/AdminDashboard.tsx web/src/components/admin/AdminDashboard.test.tsx
git commit -m "feat(web): embed Layout Studio and Telemetry into AdminDashboard tabs"
```

---

### Task 2: Streamline App Navigation & Dynamic Media Type Icons

**Files:**
* Modify: `web/src/App.tsx:1-240`
* Modify: `web/src/App.test.tsx`

**Interfaces:**
* Produces:
  * Streamlined top navigation in `App.tsx`:
    * Logo -> Home
    * Home tab
    * Dynamic user library tabs with media-type-specific icons (`Film`, `Tv`, `Sparkles`, `Music`, `Video`, `BookOpen`)
    * Admin tab (for admin accounts)
    * User profile chip & switcher
  * Removed from top nav: `Movies`, `Shows`, `Studio`, `Telemetry`.
  * Pass `onPlayItem={handlePlayItem}` to `<AdminDashboard onPlayItem={handlePlayItem} />`.

- [ ] **Step 1: Write failing tests in `App.test.tsx`**

Update `App.test.tsx` to verify:
1. Top navigation renders "Home" and the user's active libraries.
2. Hardcoded "Movies" and "Shows" buttons are NOT present in the top navigation.
3. "Studio" and "Telemetry" buttons are NOT present in the top navigation.
4. "Admin" button is present for admin users and navigates to the Admin view.

- [ ] **Step 2: Run tests to verify failure**

Run: `cd web && npm test -- --run App.test.tsx`
Expected: FAIL (assertion expects Movies / Shows / Studio / Telemetry not to be in document).

- [ ] **Step 3: Update `App.tsx` navigation**

In `web/src/App.tsx`:
1. Remove the hardcoded `<button>` elements for `movies`, `shows`, `studio`, and `telemetry`.
2. Add a dynamic icon helper for libraries:
```tsx
const getLibraryIcon = (mediaType: MediaType) => {
  switch (mediaType) {
    case 'movie':
    case 'Movie' as unknown:
      return <Film className="h-4 w-4" />;
    case 'show':
    case 'Show' as unknown:
      return <Tv className="h-4 w-4" />;
    case 'anime':
      return <Sparkles className="h-4 w-4" />;
    case 'music':
      return <Music className="h-4 w-4" />;
    case 'home_videos':
      return <Video className="h-4 w-4" />;
    case 'audiobook':
      return <BookOpen className="h-4 w-4" />;
    default:
      return <Folder className="h-4 w-4" />;
  }
};
```
3. Update `<AdminDashboard onPlayItem={handlePlayItem} />` invocation.
4. Import `Sparkles`, `Music`, `Video`, `BookOpen`, `Folder` from `lucide-react`.

- [ ] **Step 4: Run tests and verify clean build**

Run: `cd web && npm test -- --run App.test.tsx && npm test -- --run`
Run: `npm run build` in `web/`
Run: `cargo test --workspace` from repo root
Expected: 100% tests pass and build is clean.

- [ ] **Step 5: Commit**

```bash
git add web/src/App.tsx web/src/App.test.tsx
git commit -m "feat(web): streamline top navigation to user libraries only and add dynamic media icons"
```
