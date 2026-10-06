# Design Specification: Edit Library Names

**Author:** Antigravity  
**Date:** 2026-10-07  
**Status:** Approved  
**Topic:** Allow administrators to rename media libraries via REST API and inline editing in Admin Dashboard

---

## 1. Overview & Goals

Administrators currently configure media libraries with a name, media type, and monitored folder paths upon creation. However, there is no mechanism to rename an existing library once created. If a typo is made or an administrator reorganizes their collections (e.g., renaming "Movies" to "Cinema" or "Anime" to "Japanese Animation"), they must delete and recreate the library, losing playback associations and layout state.

This specification introduces:
1. `LibraryRepository::update_name` in `crates/kadr-storage` to update library names safely in SQLite.
2. `PATCH /api/v1/libraries/:id` endpoint in `crates/kadr-server` guarded by `RequireAdmin`, which also synchronizes the associated screen title in `LayoutRegistry` and broadcasts a `SystemEvent::LibraryUpdated`.
3. `api.updateLibrary` method in `web/src/api/client.ts`.
4. Inline editing UX on library cards in `AdminDashboard.tsx` with TV-friendly focus rings and keyboard shortcuts.
5. Real-time navigation tab update in `App.tsx`.

---

## 2. Architecture & Backend Design

### 2.1 Storage Layer (`crates/kadr-storage`)

- **File**: `crates/kadr-storage/src/repos/library_repo.rs`
- **Method**:
  ```rust
  pub async fn update_name(&self, id: &str, name: &str) -> Result<Library>
  ```
- **Behavior**:
  - Validates `name.trim().is_empty()`. If empty, returns `StorageError::InvalidInput("Library name cannot be empty".to_string())`.
  - Executes:
    ```sql
    UPDATE libraries SET name = ?1 WHERE id = ?2
    ```
  - Inspects affected rows. If 0 rows updated, returns `StorageError::NotFound(format!("Library {id} not found"))`.
  - Fetches and returns the refreshed `Library` struct via `self.get_by_id(id).await` (including all associated monitored paths from `library_paths`).

### 2.2 Server HTTP API (`crates/kadr-server`)

- **File**: `crates/kadr-server/src/api/library_routes.rs` & `crates/kadr-server/src/api/mod.rs`
- **Route**: `PATCH /api/v1/libraries/:id`
- **Request DTO**:
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  pub struct UpdateLibraryRequest {
      pub name: String,
  }
  ```
- **Handler**: `pub async fn update_library(...)`
  - Parameters:
    - `_admin: RequireAdmin` (enforces administrative JWT)
    - `Path(id): Path<String>`
    - `Extension(lib_repo): Extension<LibraryRepository>`
    - `Extension(registry): Extension<Arc<LayoutRegistry>>`
    - `Extension(event_bus): Extension<Arc<EventBus>>`
    - `Json(payload): Json<UpdateLibraryRequest>`
  - Logic:
    1. Trims `payload.name`. If blank, returns `StatusCode::BAD_REQUEST`.
    2. Calls `lib_repo.update_name(&id, &trimmed_name).await`.
       - If `StorageError::NotFound`, returns `StatusCode::NOT_FOUND`.
       - If `StorageError::InvalidInput`, returns `StatusCode::BAD_REQUEST`.
       - On other errors, returns `StatusCode::INTERNAL_SERVER_ERROR`.
    3. If a screen layout is registered for this library (either by matching `ScreenId::Custom(id)` or matching the library's old name):
       - Updates the screen layout's `title = trimmed_name.clone()`.
       - For any top-level `WidgetNode::Grid` whose title begins with `"All "` or matched the prior library name, updates the title to `"All <trimmed_name>"`.
    4. Publishes `SystemEvent::LibraryUpdated { library_id: id.clone(), item_count: 0, timestamp: now }` to `EventBus`.
    5. Returns `(StatusCode::OK, Json(updated_library))`.

---

## 3. Frontend Client & UI Design

### 3.1 Types & API Client (`web/`)

- **Types (`web/src/types/index.ts`)**:
  ```typescript
  export interface UpdateLibraryPayload {
    name: string;
  }
  ```
- **API Client (`web/src/api/client.ts`)**:
  ```typescript
  public async updateLibrary(id: string, payload: UpdateLibraryPayload): Promise<Library> {
    return this.request<Library>(`/api/v1/libraries/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }
  ```

### 3.2 Admin Dashboard Inline Editing (`web/src/components/admin/AdminDashboard.tsx`)

- **State Management**:
  - `editingLibraryId: string | null` (id of library currently being edited)
  - `editingName: string` (current value in the inline input)
  - `isSavingName: boolean` (loading state during API call)
- **Component Lifecycle & Interaction**:
  - **Enter Edit Mode**:
    - Click an edit button (`<button aria-label="Edit library name">` with `Pencil` icon from `lucide-react`) next to `lib.name`.
    - Or double-click the library name text.
    - Sets `editingLibraryId = lib.id` and `editingName = lib.name`.
  - **Inline Editing View**:
    - Replaces static title with an `<input>`:
      - Class: `bg-canvas border border-border-subtle text-text-main rounded-lg px-2.5 py-1 text-sm font-semibold focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`
      - Auto-focuses on mount and selects text.
      - Controlled with `editingName`.
      - On key down:
        - `Enter`: triggers `handleSaveName(lib.id)`.
        - `Escape`: cancels editing (`setEditingLibraryId(null)`).
    - Action buttons:
      - **Save**: `<button aria-label="Save library name" onClick={() => handleSaveName(lib.id)} className="p-1 text-accent hover:text-accent/80 rounded transition-colors focus-visible:ring-2 focus-visible:ring-highlight">` with `Check` icon.
      - **Cancel**: `<button aria-label="Cancel editing" onClick={() => setEditingLibraryId(null)} className="p-1 text-muted hover:text-text-main rounded transition-colors focus-visible:ring-2 focus-visible:ring-highlight">` with `X` icon.
  - **Save Handler (`handleSaveName`)**:
    - Validates `editingName.trim().length > 0`. If empty, shows error toast.
    - Dispatches `api.updateLibrary(libraryId, { name: editingName.trim() })`.
    - Updates local `libraries` state array with the returned `Library`.
    - Calls optional `onLibrariesChange?.()` callback to notify parent components.
    - Clears `editingLibraryId`.
    - Displays status message: `Library renamed to "<new_name>".`

### 3.3 Main Navigation Header Synchronization (`web/src/App.tsx`)

- Pass `onLibrariesChange={refreshLibraries}` to `<AdminDashboard />`.
- When a library is updated or renamed in `AdminDashboard`, `App` invokes `refreshLibraries()` and re-renders the navigation tabs immediately with the new library name.

---

## 4. Design System Compliance & Constraints

- **Design Tokens (`theme.md`)**:
  - `bg-canvas`, `bg-panel`, `bg-panel-hover`
  - `text-text-main`, `text-muted`, `text-accent`
  - `border-border-subtle`, `ring-highlight`
- **10-Foot TV UI Accessibility**:
  - All interactive buttons and the inline `<input>` use `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.
  - Full keyboard control (<kbd>Enter</kbd> to save, <kbd>Escape</kbd> to cancel).
- **Pure-Rust Server**:
  - Zero external native C libraries; musl and SQLite deadpool compatible.

---

## 5. Verification & Test Plan

1. **Storage Tests (`crates/kadr-storage/tests/library_test.rs`)**:
   - `update_name` modifies `name` column and persists to SQLite.
   - `update_name` with empty / whitespace string returns `StorageError::InvalidInput`.
   - `update_name` on non-existent ID returns `StorageError::NotFound`.
2. **Server API Tests (`crates/kadr-server/tests/library_routes_test.rs`)**:
   - `PATCH /api/v1/libraries/:id` requires admin auth.
   - Successful `PATCH /api/v1/libraries/:id` returns `200 OK` with updated name.
   - Updates `LayoutRegistry`'s corresponding screen layout title.
   - Publishes `SystemEvent::LibraryUpdated` on event bus.
   - Rejects blank name with `400 Bad Request`.
3. **Web Tests (`web/src/components/admin/AdminDashboard.test.tsx`)**:
   - Displays edit button for each library.
   - Clicking edit opens inline input populated with current name.
   - Canceling with Escape restores original title without API call.
   - Submitting with Enter or Save button calls `api.updateLibrary` and updates UI.
4. **Full Workspace Quality Gate**:
   - `npm test -- --run`
   - `npm run build`
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
