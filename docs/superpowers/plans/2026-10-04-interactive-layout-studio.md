# Interactive Layout Studio & Media Type Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the Layout Studio into an interactive visual AST builder where users can create, configure, reorder, delete, and persist screen layouts on the server, while expanding library media types to natively support Anime, Music, Home Videos, and Audiobooks.

**Architecture:**
* Backend: Add `save_screen`, `reset_screen`, and `delete_custom_screen` to `LayoutRegistry`, exposing `PUT /api/v1/screens/{id}`, `POST /api/v1/screens`, and `DELETE /api/v1/screens/{id}` (guarded by `RequireAdmin`), writing JSON AST overrides to `<data_dir>/screens/<id>.json`. Expand `MediaType` enum in `kadr-core`.
* Frontend: Implement `WidgetConfigModal` for visual configuration of widget types and query macro bindings; upgrade `LayoutStudio` with reorder (↑/↓), edit, delete, add widget, new screen creation, and "Save Layout" / "Reset to Default" operations; expand media types in `AdminDashboard`.

**Tech Stack:** Rust (Axum, Serde, Tokio), React 19, TypeScript, Tailwind CSS v4, Lucide React, Vitest.

## Global Constraints
* Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
* All screen mutation routes (`PUT`, `POST`, `DELETE` under `/api/v1/screens`) must be protected by `RequireAdmin`.
* Custom layouts must persist across server restarts by writing to `<data_dir>/screens/<id>.json`.
* Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `bg-cta`, `text-text-main`, `text-muted`, `border-border-subtle`).
* 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.
* 100% test pass rate across Rust workspace (`cargo test --workspace`) and frontend tests (`npm test -- --run`).

---

### Task 1: Backend MediaType Expansion, AST Persistence & Screen Mutation API

**Files:**
* Modify: `crates/kadr-core/src/models.rs:4-18`
* Modify: `crates/kadr-server/src/layout/registry.rs:18-80`
* Modify: `crates/kadr-server/src/api/screen_routes.rs:1-120`
* Test: `crates/kadr-server/tests/screen_routes_test.rs`
* Test: `crates/kadr-core/tests/media_type_test.rs`

**Interfaces:**
* Produces:
  * `MediaType` variants: `Anime`, `Music`, `HomeVideos`, `Audiobook`.
  * `LayoutRegistry::save_screen(&mut self, screen: ScreenLayout, dir: &Path) -> std::io::Result<()>`
  * `LayoutRegistry::reset_screen(&mut self, id: &ScreenId, dir: &Path) -> Result<ScreenLayout, LayoutError>`
  * `LayoutRegistry::delete_custom_screen(&mut self, id: &ScreenId, dir: &Path) -> Result<(), LayoutError>`
  * Routes in `screen_routes.rs`:
    * `PUT /api/v1/screens/{id}`: Accepts `ScreenLayout` JSON, returns `200 OK`.
    * `POST /api/v1/screens`: Accepts `{ "id": String, "title": String, "description": Option<String> }`, returns `201 Created` with `ScreenLayout`.
    * `DELETE /api/v1/screens/{id}`: Resets or deletes screen, returns `200 OK`.

- [ ] **Step 1: Write the failing tests**

In `crates/kadr-core/tests/media_type_test.rs`:
```rust
use kadr_core::models::MediaType;

#[test]
fn test_expanded_media_types_serde() {
    assert_eq!(serde_json::to_string(&MediaType::Anime).unwrap(), "\"anime\"");
    assert_eq!(serde_json::to_string(&MediaType::Music).unwrap(), "\"music\"");
    assert_eq!(serde_json::to_string(&MediaType::HomeVideos).unwrap(), "\"home_videos\"");
    assert_eq!(serde_json::to_string(&MediaType::Audiobook).unwrap(), "\"audiobook\"");

    assert_eq!(serde_json::from_str::<MediaType>("\"anime\"").unwrap(), MediaType::Anime);
    assert_eq!(serde_json::from_str::<MediaType>("\"music\"").unwrap(), MediaType::Music);
    assert_eq!(serde_json::from_str::<MediaType>("\"home_videos\"").unwrap(), MediaType::HomeVideos);
    assert_eq!(serde_json::from_str::<MediaType>("\"audiobook\"").unwrap(), MediaType::Audiobook);
}
```

In `crates/kadr-server/tests/screen_routes_test.rs`:
```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use kadr_core::ast::{ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding, QueryMacro};
use serde_json::json;
use tower::ServiceExt;

// Tests verifying PUT /api/v1/screens/{id}, POST /api/v1/screens, and DELETE /api/v1/screens/{id}
#[tokio::test]
async fn test_save_and_reset_screen_layout() {
    // 1. PUT custom layout to /api/v1/screens/home
    // 2. GET /api/v1/screens/home to verify persisted changes
    // 3. DELETE /api/v1/screens/home to verify reset to factory defaults
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test media_type_test --test screen_routes_test`
Expected: Compilation errors for missing `MediaType` variants and missing screen routes/methods.

- [ ] **Step 3: Implement MediaType expansion in `kadr-core`**

Update `crates/kadr-core/src/models.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    #[serde(alias = "Movie", alias = "MOVIE")]
    Movie,
    #[serde(alias = "Show", alias = "SHOW")]
    Show,
    #[serde(alias = "Season", alias = "SEASON")]
    Season,
    #[serde(alias = "Episode", alias = "EPISODE")]
    Episode,
    #[serde(alias = "Anime", alias = "ANIME")]
    Anime,
    #[serde(alias = "Music", alias = "MUSIC")]
    Music,
    #[serde(alias = "HomeVideos", alias = "home_videos", alias = "HOME_VIDEOS")]
    HomeVideos,
    #[serde(alias = "Audiobook", alias = "AUDIOBOOK")]
    Audiobook,
    #[default]
    #[serde(alias = "Unknown", alias = "UNKNOWN")]
    Unknown,
}
```

- [ ] **Step 4: Implement layout persistence methods in `LayoutRegistry`**

Update `crates/kadr-server/src/layout/registry.rs`:
Add `save_screen`, `reset_screen`, and `delete_custom_screen`:
```rust
impl LayoutRegistry {
    pub fn save_screen(&mut self, screen: ScreenLayout, dir: &Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let file_path = dir.join(format!("{}.json", screen.id));
        let serialized = serde_json::to_string_pretty(&screen)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&file_path, serialized)?;
        self.register_screen(screen);
        Ok(())
    }

    pub fn reset_screen(&mut self, id: &ScreenId, dir: &Path) -> Result<ScreenLayout, String> {
        let file_path = dir.join(format!("{id}.json"));
        if file_path.exists() {
            let _ = std::fs::remove_file(file_path);
        }
        match id {
            ScreenId::Home => {
                let default = default_home_layout();
                self.register_screen(default.clone());
                Ok(default)
            }
            ScreenId::Movies => {
                let default = default_movies_layout();
                self.register_screen(default.clone());
                Ok(default)
            }
            ScreenId::Shows => {
                let default = default_shows_layout();
                self.register_screen(default.clone());
                Ok(default)
            }
            ScreenId::Custom(name) => Err(format!("Custom screen '{name}' cannot be reset, use delete")),
        }
    }

    pub fn delete_custom_screen(&mut self, id: &ScreenId, dir: &Path) -> Result<(), String> {
        match id {
            ScreenId::Home | ScreenId::Movies | ScreenId::Shows => {
                Err("Built-in screens cannot be deleted".to_string())
            }
            ScreenId::Custom(_) => {
                let file_path = dir.join(format!("{id}.json"));
                if file_path.exists() {
                    let _ = std::fs::remove_file(file_path);
                }
                self.screens.remove(id);
                self.order.retain(|s| s != id);
                Ok(())
            }
        }
    }
}
```

- [ ] **Step 5: Implement `PUT`, `POST`, `DELETE` in `screen_routes.rs`**

Update `crates/kadr-server/src/api/screen_routes.rs` with `save_screen_handler`, `create_screen_handler`, and `delete_screen_handler`, guarded by `RequireAdmin`. Attach routes to screen router.

- [ ] **Step 6: Run tests and verify passing**

Run: `cargo test --workspace`
Expected: 100% tests pass cleanly.

- [ ] **Step 7: Commit**

```bash
git add crates/kadr-core crates/kadr-server
git commit -m "feat(server): add screen layout persistence APIs and expand media types"
```

---

### Task 2: Web API Client & Admin Dashboard Media Type Dropdown Expansion

**Files:**
* Modify: `web/src/types/index.ts:1-40`
* Modify: `web/src/api/client.ts:150-180`
* Modify: `web/src/components/admin/AdminDashboard.tsx:400-440`
* Test: `web/src/components/admin/AdminDashboard.test.tsx`

**Interfaces:**
* Produces:
  * `MediaType` in `web/src/types/index.ts`: `'movie' | 'show' | 'anime' | 'music' | 'home_videos' | 'audiobook' | 'unknown'`.
  * `CreateScreenPayload`: `{ id: string; title: string; description?: string }`.
  * `api.saveScreen(id: string, layout: ScreenLayout): Promise<ScreenLayout>`
  * `api.createScreen(payload: CreateScreenPayload): Promise<ScreenLayout>`
  * `api.resetScreen(id: string): Promise<ScreenLayout>`
  * `api.deleteScreen(id: string): Promise<void>`

- [ ] **Step 1: Write failing test in AdminDashboard.test.tsx**

Verify that all new media types (Anime, Music, Home Videos, Audiobooks) are rendered in the dropdown options in the Add Library modal.
Run: `cd web && npm test -- --run AdminDashboard.test.tsx`
Expected: FAIL (missing option tags).

- [ ] **Step 2: Update types & api client**

In `web/src/types/index.ts`:
```typescript
export type MediaType =
  | 'movie'
  | 'show'
  | 'anime'
  | 'music'
  | 'home_videos'
  | 'audiobook'
  | 'unknown';

export interface CreateScreenPayload {
  id: string;
  title: string;
  description?: string;
}
```

In `web/src/api/client.ts`:
```typescript
saveScreen(id: string, layout: ScreenLayout): Promise<ScreenLayout> {
  return this.request<ScreenLayout>(`/screens/${id}`, {
    method: 'PUT',
    body: JSON.stringify(layout),
  });
},

createScreen(payload: CreateScreenPayload): Promise<ScreenLayout> {
  return this.request<ScreenLayout>('/screens', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
},

resetScreen(id: string): Promise<ScreenLayout> {
  return this.request<ScreenLayout>(`/screens/${id}`, {
    method: 'DELETE',
  });
},

deleteScreen(id: string): Promise<void> {
  return this.request<void>(`/screens/${id}`, {
    method: 'DELETE',
  });
},
```

- [ ] **Step 3: Update `AdminDashboard.tsx` Media Type selector**

In `web/src/components/admin/AdminDashboard.tsx`, expand the `<select>` options in the Add Library modal:
```tsx
<option value="movie">Movies (Feature films)</option>
<option value="show">TV Shows (Episodic series)</option>
<option value="anime">Anime (Anime series & films)</option>
<option value="music">Music & Concerts (Audio albums & live concerts)</option>
<option value="home_videos">Home Videos & Clips (Personal recordings)</option>
<option value="audiobook">Audiobooks & Podcasts (Spoken word content)</option>
```

- [ ] **Step 4: Run tests and verify clean build**

Run: `cd web && npm test -- --run AdminDashboard.test.tsx && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/types/index.ts web/src/api/client.ts web/src/components/admin/AdminDashboard.tsx web/src/components/admin/AdminDashboard.test.tsx
git commit -m "feat(web): expand media types in api client and admin dashboard"
```

---

### Task 3: Widget Configurator Modal Component

**Files:**
* Create: `web/src/components/studio/WidgetConfigModal.tsx`
* Create: `web/src/components/studio/WidgetConfigModal.test.tsx`

**Interfaces:**
* Produces:
  * `WidgetConfigModalProps`:
    ```typescript
    export interface WidgetConfigModalProps {
      isOpen: boolean;
      initialWidget?: WidgetNode | null;
      libraries: Library[];
      onSave: (widget: WidgetNode) => void;
      onClose: () => void;
    }
    ```
  * Component `WidgetConfigModal`: A modal dialog styled with theme tokens (`bg-panel`, `border-border-subtle`, `text-text-main`, `bg-cta`, `ring-highlight`), allowing users to configure widget type (`hero_banner`, `carousel`, `grid`), title, columns, query macro (`recently_added`, `top_rated`, `continue_watching`, `library_items`, `genre_shelf`, `spotlight_item`), limit, and sort.

- [ ] **Step 1: Write failing component tests in `WidgetConfigModal.test.tsx`**

Test scenarios:
1. Renders with default fields when creating a new widget.
2. Changes widget type between Hero, Carousel, and Grid (showing columns input only for Grid).
3. Selects `library_items` macro and verifies library dropdown renders with passed libraries.
4. Submitting form calls `onSave` with well-formed `WidgetNode`.
5. Prefills fields when `initialWidget` is passed.
6. Escape key and cancel button call `onClose`.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run WidgetConfigModal.test.tsx`
Expected: FAIL (component does not exist).

- [ ] **Step 3: Implement `WidgetConfigModal.tsx`**

Implement `WidgetConfigModal.tsx` using Tailwind v4 theme tokens, clean accessibility (`aria-modal`, `role="dialog"`), Escape key handling, and validation.

- [ ] **Step 4: Run tests and verify passing**

Run: `cd web && npm test -- --run WidgetConfigModal.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/studio/WidgetConfigModal.tsx web/src/components/studio/WidgetConfigModal.test.tsx
git commit -m "feat(web): add WidgetConfigModal for declarative layout customization"
```

---

### Task 4: Interactive Layout Studio Canvas & Screen AST Management

**Files:**
* Modify: `web/src/components/studio/LayoutStudio.tsx:1-403`
* Modify: `web/src/components/studio/LayoutStudio.test.tsx`

**Interfaces:**
* Produces:
  * Enhanced `LayoutStudio`:
    * "+ Add Widget" button opening `WidgetConfigModal`.
    * Widget Tree cards with Move Up (↑), Move Down (↓), Edit, Delete, and Hide/Show actions.
    * "+ New Screen" modal dialog to create custom screens via `api.createScreen`.
    * "Save Layout" button calling `api.saveScreen` with dirty state indicator.
    * "Reset to Default" button calling `api.resetScreen`.
    * Immediate real-time preview updates in the viewport frame.

- [ ] **Step 1: Write failing tests in `LayoutStudio.test.tsx`**

Update `LayoutStudio.test.tsx` to verify:
1. Reordering widgets up and down updates the tree order and sets `isDirty`.
2. Clicking Edit on a widget opens `WidgetConfigModal` with existing data.
3. Adding a new widget adds a node to the tree and updates the preview.
4. Deleting a widget removes it from the tree.
5. Clicking "Save Layout" calls `api.saveScreen` and clears `isDirty`.
6. Clicking "Reset to Default" restores default layout via `api.resetScreen`.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run LayoutStudio.test.tsx`
Expected: FAIL (missing interaction elements).

- [ ] **Step 3: Implement interactive canvas and AST manager in `LayoutStudio.tsx`**

Integrate `WidgetConfigModal`, state management for `isDirty`, `editingWidget`, `isAddingWidget`, `isCreatingScreen`, and reordering/deletion helpers.

- [ ] **Step 4: Run tests and verify clean build**

Run: `cd web && npm test -- --run LayoutStudio.test.tsx && npm test -- --run`
Run: `npm run build` in `web/`
Run: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 100% PASS across all suites.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/studio/LayoutStudio.tsx web/src/components/studio/LayoutStudio.test.tsx
git commit -m "feat(web): implement interactive layout builder, reordering, and AST persistence in LayoutStudio"
```
