# Edit Library Names Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable administrators to rename media libraries via an authenticated `PATCH /api/v1/libraries/:id` endpoint and an inline editing UI on library cards in the Admin Dashboard, with automatic screen title and navigation synchronization.

**Architecture:** Add `LibraryRepository::update_name` in SQLite storage; add authenticated `PATCH /api/v1/libraries/:id` in the Axum server to update the library, sync `LayoutRegistry`'s screen layout title, and publish `SystemEvent::LibraryUpdated`; and implement inline editing on the React frontend using design tokens from `theme.md` and 10-foot TV UI focus rings, notifying `App.tsx` to refresh navigation tabs.

**Tech Stack:** Rust (axum, rusqlite, deadpool-sqlite, tokio), TypeScript, React 19, Tailwind CSS, Lucide icons, Vitest, Testing Library.

## Global Constraints

- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- Endpoints modifying libraries require administrative credentials (`RequireAdmin`).
- Empty or whitespace-only library names must be rejected with 400 Bad Request.
- Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `ring-highlight`, `border-border-subtle`).
- 10-foot TV UI focus rings: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all interactive inputs and buttons.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.
- Clippy (`cargo clippy --workspace --all-targets -- -D warnings`) must remain clean with 0 warnings.

---

### Task 1: Storage Layer `LibraryRepository::update_name` (`crates/kadr-storage`)

**Files:**
- Modify: `crates/kadr-storage/src/repos/library_repo.rs:245-255`
- Test: `crates/kadr-storage/tests/repositories_test.rs:480-506`

**Interfaces:**
- Consumes: `deadpool_sqlite::Pool`, `kadr_core::models::Library`, `crates/kadr-storage/src/error::{Result, StorageError}`
- Produces: `LibraryRepository::update_name(&self, id: &str, name: &str) -> Result<Library>`

- [ ] **Step 1: Write failing unit tests for `update_name`**

In `crates/kadr-storage/tests/repositories_test.rs`, add a dedicated test `test_library_update_name`:

```rust
#[tokio::test]
async fn test_library_update_name() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());

    let lib = lib_repo
        .create(
            "lib-orig",
            "Original Name",
            "/media/movies",
            kadr_core::models::MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    // 1. Successful update
    let updated = lib_repo.update_name(&lib.id, "Renamed Movies").await.unwrap();
    assert_eq!(updated.name, "Renamed Movies");
    assert_eq!(updated.id, "lib-orig");

    let fetched = lib_repo.get_by_id(&lib.id).await.unwrap().expect("library should exist");
    assert_eq!(fetched.name, "Renamed Movies");

    // 2. Reject empty or whitespace-only name
    let empty_err = lib_repo.update_name(&lib.id, "   ").await.unwrap_err();
    match empty_err {
        kadr_storage::error::StorageError::InvalidInput(msg) => {
            assert!(msg.to_lowercase().contains("empty"));
        }
        other => panic!("Expected InvalidInput error, got: {:?}", other),
    }

    // 3. Return NotFound on non-existent id
    let not_found_err = lib_repo.update_name("non-existent-lib", "New Name").await.unwrap_err();
    match not_found_err {
        kadr_storage::error::StorageError::NotFound(_) => {}
        other => panic!("Expected NotFound error, got: {:?}", other),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-storage --test repositories_test test_library_update_name`
Expected: FAIL with "no method named `update_name` found for struct `LibraryRepository`"

- [ ] **Step 3: Implement `update_name` in `LibraryRepository`**

In `crates/kadr-storage/src/repos/library_repo.rs`, add the `update_name` method:

```rust
    pub async fn update_name(&self, id: &str, name: &str) -> Result<Library> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(StorageError::InvalidInput(
                "Library name cannot be empty".to_string(),
            ));
        }

        let id = id.to_string();
        let trimmed_name = trimmed.to_string();
        let conn = self.pool.get().await?;
        let rows_affected = conn
            .interact(move |c| {
                let affected = c.execute(
                    "UPDATE libraries SET name = ?1 WHERE id = ?2",
                    params![trimmed_name, id],
                )?;
                Ok(affected)
            })
            .await?;

        if rows_affected == 0 {
            return Err(StorageError::NotFound(format!("Library {id} not found")));
        }

        self.get_by_id(&id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Library {id} not found")))
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kadr-storage --test repositories_test test_library_update_name`
Expected: PASS

- [ ] **Step 5: Run storage test suite and clippy**

Run: `cargo test -p kadr-storage && cargo clippy -p kadr-storage -- -D warnings`
Expected: 0 errors, 0 warnings

- [ ] **Step 6: Commit changes**

```bash
git add crates/kadr-storage/src/repos/library_repo.rs crates/kadr-storage/tests/repositories_test.rs
git commit -m "feat(storage): add LibraryRepository::update_name method"
```

---

### Task 2: Server API `PATCH /api/v1/libraries/:id` (`crates/kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/api/library_routes.rs:70-85, 330-365`
- Modify: `crates/kadr-server/src/api/mod.rs:160-180`
- Test: `crates/kadr-server/tests/library_routes_test.rs:370-382`

**Interfaces:**
- Consumes: `LibraryRepository::update_name`, `LayoutRegistry`, `EventBus`, `RequireAdmin`
- Produces: `PATCH /api/v1/libraries/:id` returning `200 OK` with updated `Library` JSON

- [ ] **Step 1: Write failing integration tests for `PATCH /api/v1/libraries/:id`**

In `crates/kadr-server/tests/library_routes_test.rs`, add `test_update_library_name`:

```rust
#[tokio::test]
async fn test_update_library_name() {
    let ctx = setup_test_context().await;

    // Create a library via POST
    let create_payload = json!({
        "name": "Original Name",
        "path": "/media/movies",
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let created_lib: Value = serde_json::from_slice(&body_bytes).unwrap();
    let lib_id = created_lib["id"].as_str().unwrap();

    // 1. Standard user cannot rename library (403 Forbidden)
    let patch_payload = json!({ "name": "Standard Renamed" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/libraries/{}", lib_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.standard_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_payload).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 2. Admin successfully renames library (200 OK)
    let patch_payload = json!({ "name": "Cinema Classics" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/libraries/{}", lib_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_payload).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let updated_lib: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(updated_lib["name"], "Cinema Classics");
    assert_eq!(updated_lib["id"], lib_id);

    // 3. Reject empty name (400 Bad Request)
    let bad_payload = json!({ "name": "   " });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/libraries/{}", lib_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&bad_payload).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 4. Non-existent library (404 Not Found)
    let req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/libraries/non-existent-uuid")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_payload).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test library_routes_test test_update_library_name`
Expected: FAIL with 405 Method Not Allowed or 404 Not Found

- [ ] **Step 3: Implement `UpdateLibraryRequest` and `update_library` handler in `crates/kadr-server/src/api/library_routes.rs`**

In `crates/kadr-server/src/api/library_routes.rs`:
Add the request struct:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLibraryRequest {
    pub name: String,
}
```

Add the handler:
```rust
/// Handler for `PATCH /api/v1/libraries/{id}`.
///
/// Updates the library name, synchronizes associated screen layout title,
/// and broadcasts a LibraryUpdated event. Requires admin role.
pub async fn update_library(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(layout_registry): Extension<Arc<LayoutRegistry>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<UpdateLibraryRequest>,
) -> Result<Json<Library>, StatusCode> {
    let trimmed_name = payload.name.trim();
    if trimmed_name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let updated = match lib_repo.update_name(&id, trimmed_name).await {
        Ok(lib) => lib,
        Err(StorageError::NotFound(_)) => return Err(StatusCode::NOT_FOUND),
        Err(StorageError::InvalidInput(_)) => return Err(StatusCode::BAD_REQUEST),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to update library name");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Synchronize associated screen layout if one exists
    let screen_id = id.parse::<kadr_core::ast::ScreenId>().unwrap();
    if let Some(mut screen) = layout_registry.get_screen(&screen_id) {
        screen.title = trimmed_name.to_string();
        for widget in &mut screen.widgets {
            if let kadr_core::ast::WidgetNode::Grid { title, .. } = widget {
                if title.starts_with("All ") {
                    *title = format!("All {trimmed_name}");
                }
            }
        }
        layout_registry.register_screen(screen);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: id,
        item_count: 0,
        timestamp: now,
    });

    Ok(Json(updated))
}
```

- [ ] **Step 4: Register route in `crates/kadr-server/src/api/mod.rs`**

In `crates/kadr-server/src/api/mod.rs`, update the `/api/v1/libraries/:id` route definition:
```rust
        .route(
            "/api/v1/libraries/:id",
            patch(library_routes::update_library)
                .delete(library_routes::delete_library),
        )
```

- [ ] **Step 5: Run integration tests to verify they pass**

Run: `cargo test -p kadr-server --test library_routes_test test_update_library_name`
Expected: PASS

- [ ] **Step 6: Run full workspace test suite and clippy**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings

- [ ] **Step 7: Commit changes**

```bash
git add crates/kadr-server/src/api/library_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/tests/library_routes_test.rs
git commit -m "feat(server): add PATCH /api/v1/libraries/:id to rename media library"
```

---

### Task 3: Web Client API & Admin Dashboard Inline Editing (`web/`)

**Files:**
- Modify: `web/src/types/index.ts:310-325`
- Modify: `web/src/api/client.ts:480-495`
- Modify: `web/src/components/admin/AdminDashboard.tsx:40-60, 420-450`
- Modify: `web/src/App.tsx:270-275`
- Test: `web/src/components/admin/AdminDashboard.test.tsx`

**Interfaces:**
- Consumes: `api.updateLibrary(id, { name })`
- Produces: Inline editing UI on library cards in `AdminDashboard.tsx`, notifying `App.tsx` via `onLibrariesChange` callback

- [ ] **Step 1: Add types in `web/src/types/index.ts` and client method in `web/src/api/client.ts`**

In `web/src/types/index.ts`:
```typescript
export interface UpdateLibraryPayload {
  name: string;
}
```

In `web/src/api/client.ts`:
```typescript
  public async updateLibrary(id: string, payload: UpdateLibraryPayload): Promise<Library> {
    return this.request<Library>(`/api/v1/libraries/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }
```

- [ ] **Step 2: Write failing component tests in `web/src/components/admin/AdminDashboard.test.tsx`**

In `web/src/components/admin/AdminDashboard.test.tsx`, add test suite:

```typescript
  describe('Inline Library Rename', () => {
    it('renders edit button and toggles inline input with keyboard shortcuts', async () => {
      vi.spyOn(api, 'updateLibrary').mockImplementation(async (id, payload) => ({
        ...mockLibraries[0],
        id,
        name: payload.name,
      }));

      render(<AdminDashboard />);

      await waitFor(() => {
        expect(screen.getByText('Featured Movies')).toBeInTheDocument();
      });

      // Find edit button for first library
      const editBtn = screen.getByRole('button', { name: /edit library name/i });
      expect(editBtn).toBeInTheDocument();

      // Click to enter inline edit mode
      fireEvent.click(editBtn);

      const input = screen.getByDisplayValue('Featured Movies');
      expect(input).toBeInTheDocument();

      // Cancel with Escape
      fireEvent.keyDown(input, { key: 'Escape', code: 'Escape' });
      expect(screen.queryByDisplayValue('Featured Movies')).not.toBeInTheDocument();
      expect(screen.getByText('Featured Movies')).toBeInTheDocument();
      expect(api.updateLibrary).not.toHaveBeenCalled();

      // Re-enter and save with Enter
      fireEvent.click(screen.getByRole('button', { name: /edit library name/i }));
      const editInput = screen.getByDisplayValue('Featured Movies');
      fireEvent.change(editInput, { target: { value: 'Blockbuster Cinema' } });
      fireEvent.keyDown(editInput, { key: 'Enter', code: 'Enter' });

      await waitFor(() => {
        expect(api.updateLibrary).toHaveBeenCalledWith('lib-1', { name: 'Blockbuster Cinema' });
        expect(screen.getByText('Blockbuster Cinema')).toBeInTheDocument();
      });
    });

    it('saves library name when clicking the save check button', async () => {
      vi.spyOn(api, 'updateLibrary').mockImplementation(async (id, payload) => ({
        ...mockLibraries[0],
        id,
        name: payload.name,
      }));

      const onLibrariesChange = vi.fn();
      render(<AdminDashboard onLibrariesChange={onLibrariesChange} />);

      await waitFor(() => {
        expect(screen.getByText('Featured Movies')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByRole('button', { name: /edit library name/i }));
      const editInput = screen.getByDisplayValue('Featured Movies');
      fireEvent.change(editInput, { target: { value: 'Criterion Collection' } });

      const saveBtn = screen.getByRole('button', { name: /save library name/i });
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(api.updateLibrary).toHaveBeenCalledWith('lib-1', { name: 'Criterion Collection' });
        expect(screen.getByText('Criterion Collection')).toBeInTheDocument();
        expect(onLibrariesChange).toHaveBeenCalled();
      });
    });
  });
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd web && npm test -- --run AdminDashboard.test.tsx`
Expected: FAIL with "Unable to find an accessible element with the role "button" and name `/edit library name/i`"

- [ ] **Step 4: Implement inline editing in `AdminDashboard.tsx` and wire in `App.tsx`**

In `web/src/components/admin/AdminDashboard.tsx`:
1. Add props interface:
```typescript
interface AdminDashboardProps {
  onPlayItem?: (itemId: number) => void;
  onLibrariesChange?: () => void;
  initialTab?: AdminTab;
}
```
2. Add state:
```typescript
  const [editingLibraryId, setEditingLibraryId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState<string>('');
  const [isSavingName, setIsSavingName] = useState<boolean>(false);
```
3. Add handler:
```typescript
  const handleStartEdit = (lib: Library) => {
    setEditingLibraryId(lib.id);
    setEditingName(lib.name);
  };

  const handleCancelEdit = () => {
    setEditingLibraryId(null);
    setEditingName('');
  };

  const handleSaveName = async (libraryId: string) => {
    const trimmed = editingName.trim();
    if (!trimmed) {
      showStatus('error', 'Library name cannot be empty.');
      return;
    }

    const currentLib = libraries.find((l) => l.id === libraryId);
    if (currentLib && currentLib.name === trimmed) {
      handleCancelEdit();
      return;
    }

    setIsSavingName(true);
    try {
      const updated = await api.updateLibrary(libraryId, { name: trimmed });
      setLibraries((prev) => prev.map((l) => (l.id === libraryId ? updated : l)));
      handleCancelEdit();
      onLibrariesChange?.();
      showStatus('success', `Library renamed to "${updated.name}".`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to rename library';
      showStatus('error', msg);
    } finally {
      setIsSavingName(false);
    }
  };
```
4. In JSX card header:
Import `Pencil`, `Check`, `X` from `lucide-react`.
Replace static title with conditional inline editing:
```tsx
  <div className="flex items-center gap-2 flex-1 min-w-0 mr-2">
    {lib.media_type === 'Movie' || lib.media_type === 'movie' ? (
      <Film className="w-5 h-5 text-accent shrink-0" />
    ) : (
      <Tv className="w-5 h-5 text-accent shrink-0" />
    )}
    {editingLibraryId === lib.id ? (
      <div className="flex items-center gap-1.5 flex-1 min-w-0">
        <input
          type="text"
          value={editingName}
          disabled={isSavingName}
          onChange={(e) => setEditingName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              handleSaveName(lib.id);
            } else if (e.key === 'Escape') {
              e.preventDefault();
              handleCancelEdit();
            }
          }}
          autoFocus
          className="bg-canvas border border-border-subtle text-text-main rounded-lg px-2.5 py-1 text-sm font-semibold focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none w-full max-w-[240px]"
        />
        <button
          type="button"
          onClick={() => handleSaveName(lib.id)}
          disabled={isSavingName}
          aria-label="Save library name"
          title="Save (Enter)"
          className="p-1 text-accent hover:text-accent/80 hover:bg-accent/10 rounded transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight shrink-0"
        >
          <Check className="w-4 h-4" />
        </button>
        <button
          type="button"
          onClick={handleCancelEdit}
          disabled={isSavingName}
          aria-label="Cancel editing"
          title="Cancel (Esc)"
          className="p-1 text-muted hover:text-text-main hover:bg-panel rounded transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight shrink-0"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    ) : (
      <div className="flex items-center gap-2 min-w-0">
        <h3
          onDoubleClick={() => handleStartEdit(lib)}
          className="font-semibold text-lg text-text-main truncate cursor-pointer"
          title="Double-click to rename"
        >
          {lib.name}
        </h3>
        <button
          type="button"
          onClick={() => handleStartEdit(lib)}
          aria-label="Edit library name"
          title="Rename library"
          className="p-1 text-muted hover:text-accent hover:bg-panel rounded transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight"
        >
          <Pencil className="w-3.5 h-3.5" />
        </button>
      </div>
    )}
  </div>
```
5. In `web/src/App.tsx`:
Pass `onLibrariesChange={refreshLibraries}`:
```tsx
<AdminDashboard onPlayItem={handlePlayItem} onLibrariesChange={refreshLibraries} />
```

- [ ] **Step 5: Run component tests to verify they pass**

Run: `cd web && npm test -- --run AdminDashboard.test.tsx`
Expected: PASS

- [ ] **Step 6: Run full frontend test suite and build**

Run: `cd web && npm test -- --run && npm run build`
Expected: PASS and clean build

- [ ] **Step 7: Commit changes**

```bash
git add web/src/types/index.ts web/src/api/client.ts web/src/components/admin/AdminDashboard.tsx web/src/components/admin/AdminDashboard.test.tsx web/src/App.tsx
git commit -m "feat(web): add inline editing for library names in AdminDashboard and client API"
```

---

### Task 4: Full System Verification & Quality Gate

**Files:**
- N/A (Workspace verification)

**Interfaces:**
- Workspace quality gate verification

- [ ] **Step 1: Run full Rust workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Run Rust clippy across all targets**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 errors, 0 warnings

- [ ] **Step 3: Run full web test suite**

Run: `cd web && npm test -- --run`
Expected: All tests pass

- [ ] **Step 4: Run production web build**

Run: `cd web && npm run build`
Expected: Clean build into `web/dist`

- [ ] **Step 5: Verify live server restart and behavior**

Verify backend responds properly and serves updated static assets.
