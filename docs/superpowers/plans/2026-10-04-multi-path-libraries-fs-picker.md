# Filesystem Selector & Multi-Path Media Libraries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide an interactive server-side filesystem directory picker in the web Admin interface for selecting media directories without manual typing, and allow media libraries to span multiple storage paths.

**Architecture:** A normalized SQLite table `library_paths` tracks one or more folder paths per library. The server provides a secure, admin-authenticated filesystem navigation endpoint (`GET /api/v1/system/fs`) that inspects directory trees and provides shortcut jump points. The web frontend replaces raw path text inputs with an interactive `FolderPickerModal` and enables managing multi-path configurations on both new and existing libraries.

**Tech Stack:** Rust (Axum, SQLite via rusqlite & deadpool, tokio fs, notify), TypeScript (React 19, Tailwind CSS v4, Lucide icons, Vitest).

## Global Constraints

- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- SQLite operations must maintain WAL mode, `PRAGMA foreign_keys = ON`, `PRAGMA synchronous = NORMAL`, and `PRAGMA busy_timeout = 5000`.
- Filesystem browsing API (`/api/v1/system/fs`) must be strictly guarded by `RequireAdmin`.
- Filesystem traversal must filter out hidden directories (starting with `.`) and only list directories (`is_dir()`).
- Libraries must maintain at least one storage directory; removing the last path must return 400 Bad Request.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.

---

### Task 1: Database Migration `006_multiple_library_paths.sql` & Storage Layer Multi-Path Operations

**Files:**
- Create: `crates/kadr-storage/src/migrations/006_multiple_library_paths.sql`
- Modify: `crates/kadr-core/src/models.rs:20-46`
- Modify: `crates/kadr-storage/src/migrations.rs:1-18`
- Modify: `crates/kadr-storage/src/repos/library_repo.rs:1-182`
- Test: `crates/kadr-storage/tests/multiple_library_paths_test.rs`

**Interfaces:**
- Consumes: `Library` model, SQLite connection pool.
- Produces:
  - `Library.paths: Vec<PathBuf>` on `kadr_core::models::Library`.
  - `LibraryRepository::create_with_paths(&self, id: &str, name: &str, paths: &[PathBuf], media_type: MediaType, is_private: bool, pin_hash: Option<&str>) -> Result<Library>`.
  - `LibraryRepository::add_path(&self, library_id: &str, path: impl AsRef<std::path::Path>) -> Result<()>`.
  - `LibraryRepository::remove_path(&self, library_id: &str, path: impl AsRef<std::path::Path>) -> Result<()>`.
  - Updated `LibraryRepository::get_by_id` and `get_all` populating `paths`.

- [ ] **Step 1: Write the failing storage integration test**

```rust
// crates/kadr-storage/tests/multiple_library_paths_test.rs
use deadpool_sqlite::{Config, Runtime};
use kadr_core::models::MediaType;
use kadr_storage::migrations::run_migrations;
use kadr_storage::repos::LibraryRepository;
use std::path::PathBuf;

async fn setup_test_db() -> deadpool_sqlite::Pool {
    let cfg = Config::new("");
    let pool = cfg.create_pool(Runtime::Tokio1).unwrap();
    let mut conn = pool.get().await.unwrap();
    conn.interact(|c| run_migrations(c)).await.unwrap().unwrap();
    pool
}

#[tokio::test]
async fn test_create_and_manage_multiple_library_paths() {
    let pool = setup_test_db().await;
    let repo = LibraryRepository::new(pool);

    let paths = vec![
        PathBuf::from("/media/movies1"),
        PathBuf::from("/media/movies2"),
    ];

    let lib = repo
        .create_with_paths(
            "lib-multi",
            "Multi Movies",
            &paths,
            MediaType::Movie,
            false,
            None,
        )
        .await
        .expect("Failed to create library with multiple paths");

    assert_eq!(lib.id, "lib-multi");
    assert_eq!(lib.path, PathBuf::from("/media/movies1"));
    assert_eq!(lib.paths.len(), 2);
    assert_eq!(lib.paths[0], PathBuf::from("/media/movies1"));
    assert_eq!(lib.paths[1], PathBuf::from("/media/movies2"));

    // Fetch by id
    let fetched = repo.get_by_id("lib-multi").await.unwrap().expect("Library should exist");
    assert_eq!(fetched.paths.len(), 2);

    // Add a 3rd path
    repo.add_path("lib-multi", "/media/movies3")
        .await
        .expect("Failed to add 3rd path");

    let updated = repo.get_by_id("lib-multi").await.unwrap().unwrap();
    assert_eq!(updated.paths.len(), 3);
    assert!(updated.paths.contains(&PathBuf::from("/media/movies3")));

    // Remove 2nd path
    repo.remove_path("lib-multi", "/media/movies2")
        .await
        .expect("Failed to remove path");

    let after_remove = repo.get_by_id("lib-multi").await.unwrap().unwrap();
    assert_eq!(after_remove.paths.len(), 2);
    assert!(!after_remove.paths.contains(&PathBuf::from("/media/movies2")));

    // Removing non-existent path errors or succeeds idempotently
    repo.remove_path("lib-multi", "/media/movies3").await.unwrap();
    let final_lib = repo.get_by_id("lib-multi").await.unwrap().unwrap();
    assert_eq!(final_lib.paths.len(), 1);

    // Attempting to remove the last path must fail
    let err = repo.remove_path("lib-multi", "/media/movies1").await;
    assert!(err.is_err(), "Removing last path must fail");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test multiple_library_paths_test`
Expected: FAIL with "no method named `create_with_paths`" or missing `paths` field.

- [ ] **Step 3: Update `Library` model in `crates/kadr-core/src/models.rs`**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    pub media_type: MediaType,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default, skip_serializing)]
    pub pin_hash: Option<String>,
    pub created_at: i64,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            path: PathBuf::new(),
            paths: Vec::new(),
            media_type: MediaType::Unknown,
            is_private: false,
            pin_hash: None,
            created_at: 0,
        }
    }
}
```

- [ ] **Step 4: Create migration `006_multiple_library_paths.sql` and register it**

In `crates/kadr-storage/src/migrations/006_multiple_library_paths.sql`:
```sql
CREATE TABLE IF NOT EXISTS library_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(library_id, path)
);

-- Seed existing single library paths into library_paths table
INSERT OR IGNORE INTO library_paths (library_id, path, created_at)
SELECT id, path, created_at FROM libraries WHERE path IS NOT NULL AND path != '';

CREATE INDEX IF NOT EXISTS idx_library_paths_lib ON library_paths(library_id);
CREATE INDEX IF NOT EXISTS idx_library_paths_path ON library_paths(path);
```

In `crates/kadr-storage/src/migrations.rs`:
Add `M::up(include_str!("migrations/006_multiple_library_paths.sql")),` to `migrations()`.

- [ ] **Step 5: Implement multi-path methods in `LibraryRepository`**

In `crates/kadr-storage/src/repos/library_repo.rs`:
- Implement `create_with_paths`.
- Update `create` to delegate to `create_with_paths` with `&[path.as_ref().to_path_buf()]`.
- In `insert`, insert into `libraries` and insert `lib.paths` (or `[lib.path]` if `paths` is empty) into `library_paths`.
- Implement `add_path(&self, library_id: &str, path: impl AsRef<std::path::Path>) -> Result<()>`.
- Implement `remove_path(&self, library_id: &str, path: impl AsRef<std::path::Path>) -> Result<()>` checking that `COUNT(*) > 1`. If remaining count is 1 or fewer, return an error. Also if deleting the path matching `libraries.path`, update `libraries.path` to another remaining path.
- In `get_all` and `get_by_id`, load paths from `library_paths` for each library. If `library_paths` is empty, fallback to `vec![lib.path.clone()]`.

- [ ] **Step 6: Run storage tests to verify passing**

Run: `cargo test --test multiple_library_paths_test` and `cargo test -p kadr-storage`
Expected: PASS with 100% passing tests.

- [ ] **Step 7: Commit changes**

```bash
git add crates/kadr-core/src/models.rs crates/kadr-storage/
git commit -m "feat(storage): add multiple_library_paths migration and LibraryRepo multi-path CRUD"
```

---

### Task 2: Server Filesystem Browsing API, Multi-Path Library Routes & Scanner Integration

**Files:**
- Create: `crates/kadr-server/src/api/system_routes.rs`
- Modify: `crates/kadr-server/src/api/library_routes.rs:24-331`
- Modify: `crates/kadr-server/src/api/mod.rs:1-180`
- Modify: `crates/kadr-ingest/src/watcher/fs_watcher.rs:32-64`
- Test: `crates/kadr-server/tests/filesystem_routes_test.rs`
- Test: `crates/kadr-server/tests/multi_path_library_routes_test.rs`

**Interfaces:**
- Consumes: `RequireAdmin` extractor, `LibraryRepository`, `tokio::fs`.
- Produces:
  - `GET /api/v1/system/fs?path=...` -> `FsBrowseResponse { current_path, parent_path, directories, shortcuts }`.
  - `POST /api/v1/libraries/{id}/paths` -> `201 Created` or `200 OK`.
  - `DELETE /api/v1/libraries/{id}/paths?path=...` -> `200 OK` or `400 Bad Request`.
  - `POST /api/v1/libraries` supporting `paths: Option<Vec<PathBuf>>`.
  - Ingestion scanning across all paths in `library.paths`.

- [ ] **Step 1: Write failing tests for filesystem browsing and multi-path routes**

In `crates/kadr-server/tests/filesystem_routes_test.rs`:
```rust
use axum::http::{Request, StatusCode};
use kadr_server::api::create_router_with_events;
use tower::ServiceExt;

// Helper to construct test router with admin auth
#[tokio::test]
async fn test_browse_filesystem_requires_admin() {
    // Non-admin request returns 401/403
}

#[tokio::test]
async fn test_browse_filesystem_returns_directories_and_shortcuts() {
    // Admin request with path="/" returns directories and shortcuts
}
```

In `crates/kadr-server/tests/multi_path_library_routes_test.rs`:
```rust
// Test POST /api/v1/libraries with paths array
// Test POST /api/v1/libraries/{id}/paths to add path
// Test DELETE /api/v1/libraries/{id}/paths to remove path
// Test DELETE last path returns 400 Bad Request
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test filesystem_routes_test --test multi_path_library_routes_test`
Expected: FAIL.

- [ ] **Step 3: Implement `system_routes.rs` for filesystem browsing**

Create `crates/kadr-server/src/api/system_routes.rs`:
- Define `FsDirectoryEntry { name: String, path: String }`.
- Define `FsShortcut { name: String, path: String }`.
- Define `FsBrowseResponse { current_path: String, parent_path: Option<String>, directories: Vec<FsDirectoryEntry>, shortcuts: Vec<FsShortcut> }`.
- Implement `pub async fn browse_filesystem(_admin: RequireAdmin, Query(params): Query<FsBrowseQuery>) -> Result<Json<FsBrowseResponse>, (StatusCode, Json<serde_json::Value>)>`:
  - Default `path` to `/`.
  - Canonicalize `path` and check `path.is_dir()`. Return 400 Bad Request if not found or not a directory.
  - Read directory via `tokio::fs::read_dir`. Filter entries where `file_type.is_dir()`. Filter out hidden names starting with `.`. Sort alphabetically.
  - Compute `parent_path` if `path != "/"`.
  - Build shortcuts array for existing paths among `[("/", "Root (/)"), ("/media", "Media"), ("/mnt", "Mounts"), ("/home", "Home"), ("/var", "Var")]`.

- [ ] **Step 4: Update `library_routes.rs` for multi-path support**

In `crates/kadr-server/src/api/library_routes.rs`:
- Update `CreateLibraryRequest`:
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  pub struct CreateLibraryRequest {
      pub name: String,
      pub path: Option<PathBuf>,
      pub paths: Option<Vec<PathBuf>>,
      pub media_type: MediaType,
      #[serde(default)]
      pub is_private: bool,
      #[serde(default)]
      pub pin: Option<String>,
  }
  ```
- In `create_library`:
  - Consolidate `paths`: if `payload.paths` is present and non-empty, use it. Otherwise, use `payload.path.into_iter().collect()`. If empty, return `400 Bad Request`.
  - Validate that all specified paths exist and are directories on disk.
  - Call `lib_repo.create_with_paths`.
  - Scan all existing paths in the background task.
- Add handler `add_library_path`:
  - Handler for `POST /api/v1/libraries/{id}/paths`.
  - Payload: `AddPathRequest { path: PathBuf }`.
  - Validates `path.is_dir()`.
  - Calls `lib_repo.add_path(&id, &path)`.
  - Triggers background scan on the newly added path.
- Add handler `remove_library_path`:
  - Handler for `DELETE /api/v1/libraries/{id}/paths`.
  - Query: `RemovePathQuery { path: PathBuf }`.
  - Calls `lib_repo.remove_path(&id, &path)`. If error indicating last remaining path, return `StatusCode::BAD_REQUEST`.
- In `scan_library`:
  - Iterate over all paths in `library.paths` (or `[library.path]` if `paths` is empty) and scan each directory.

- [ ] **Step 5: Update `kadr-ingest` directory watcher**

In `crates/kadr-ingest/src/watcher/fs_watcher.rs`:
- In `start_library_watcher`:
  - Watch each path in `library.paths` if it exists:
    ```rust
    let paths = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };
    for p in &paths {
        if p.exists() {
            let _ = watcher.watch(p, RecursiveMode::Recursive);
        }
    }
    ```
  - For initial scan task:
    ```rust
    for p in &paths {
        if p.exists() {
            let initial_files = scan_directory_recursive(p);
            for file in initial_files {
                if let Ok(Some((item, subs))) = pipe_clone.process_file(&lib_clone, &file).await {
                    let _ = tx_clone.send(IngestMessage::Upsert(item, subs)).await;
                }
            }
        }
    }
    ```

- [ ] **Step 6: Register routes in `crates/kadr-server/src/api/mod.rs`**

- Add `pub mod system_routes;`.
- Register:
  - `.route("/api/v1/system/fs", get(system_routes::browse_filesystem))`
  - `.route("/api/v1/libraries/{id}/paths", post(library_routes::add_library_path))`
  - `.route("/api/v1/libraries/{id}/paths", delete(library_routes::remove_library_path))`

- [ ] **Step 7: Run server tests to verify passing**

Run: `cargo test -p kadr-server` and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 100% passing tests and 0 warnings.

- [ ] **Step 8: Commit changes**

```bash
git add crates/kadr-server/ crates/kadr-ingest/
git commit -m "feat(server): add fs browsing API, multi-path library routes and ingest scanner integration"
```

---

### Task 3: Web UI FolderPickerModal & Multi-Path Library Management

**Files:**
- Create: `web/src/components/admin/FolderPickerModal.tsx`
- Create: `web/src/components/admin/FolderPickerModal.test.tsx`
- Modify: `web/src/types/index.ts:225-245`
- Modify: `web/src/api/client.ts:413-455`
- Modify: `web/src/components/admin/AdminDashboard.tsx:28-630`
- Modify: `web/src/components/admin/AdminDashboard.test.tsx`

**Interfaces:**
- Consumes: `api.browseFilesystem`, `api.createLibrary`, `api.addLibraryPath`, `api.removeLibraryPath`.
- Produces:
  - `FolderPickerModal` component: interactive server folder navigator with breadcrumbs, shortcuts, folder selection, and cancellation.
  - Multi-path folder picker in "Add Library" modal (removes manual text path input).
  - Multi-path display and management on existing library cards.

- [ ] **Step 1: Write failing component tests for `FolderPickerModal` and `AdminDashboard` multi-pathing**

In `web/src/components/admin/FolderPickerModal.test.tsx`:
```tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { FolderPickerModal } from './FolderPickerModal';
import { api } from '../../api/client';

describe('FolderPickerModal', () => {
  beforeEach(() => {
    vi.spyOn(api, 'browseFilesystem').mockResolvedValue({
      current_path: '/media',
      parent_path: '/',
      directories: [
        { name: 'movies', path: '/media/movies' },
        { name: 'shows', path: '/media/shows' },
      ],
      shortcuts: [
        { name: 'Root (/)', path: '/' },
        { name: 'Media', path: '/media' },
      ],
    });
  });

  it('renders breadcrumbs and directory list', async () => {
    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('movies')).toBeInTheDocument();
      expect(screen.getByText('shows')).toBeInTheDocument();
    });
  });

  it('calls onSelect with selected directory path', async () => {
    const onSelect = vi.fn();
    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={onSelect} />);
    await waitFor(() => expect(screen.getByText('movies')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Select This Folder/i }));
    expect(onSelect).toHaveBeenCalledWith('/media');
  });
});
```

In `web/src/components/admin/AdminDashboard.test.tsx`:
- Add test verifying Add Library modal renders "+ Add Server Folder" button instead of text input.
- Add test verifying adding multiple folder paths and displaying folder chips.
- Add test verifying library cards display multiple paths with remove buttons.

- [ ] **Step 2: Run frontend tests to verify failure**

Run: `cd web && npm test -- --run FolderPickerModal.test.tsx`
Expected: FAIL with missing module.

- [ ] **Step 3: Update `web/src/types/index.ts`**

```ts
export interface FsDirectoryEntry {
  name: string;
  path: string;
}

export interface FsShortcut {
  name: string;
  path: string;
}

export interface FsBrowseResponse {
  current_path: string;
  parent_path: string | null;
  directories: FsDirectoryEntry[];
  shortcuts: FsShortcut[];
}

export interface Library {
  id: string;
  name: string;
  path: string;
  paths?: string[];
  media_type: 'Movie' | 'Episode' | 'Show' | 'Music' | 'Other';
  is_private: boolean;
  created_at: number;
}

export interface CreateLibraryPayload {
  name: string;
  path?: string;
  paths?: string[];
  media_type: 'Movie' | 'Episode';
  is_private?: boolean;
  pin?: string;
}
```

- [ ] **Step 4: Update `web/src/api/client.ts`**

Add:
```ts
public async browseFilesystem(path?: string): Promise<FsBrowseResponse> {
  const query = path ? `?path=${encodeURIComponent(path)}` : '';
  return this.request<FsBrowseResponse>(`/api/v1/system/fs${query}`);
}

public async addLibraryPath(libraryId: string, path: string): Promise<void> {
  await this.request<void>(`/api/v1/libraries/${encodeURIComponent(libraryId)}/paths`, {
    method: 'POST',
    body: JSON.stringify({ path }),
  });
}

public async removeLibraryPath(libraryId: string, path: string): Promise<void> {
  await this.request<void>(
    `/api/v1/libraries/${encodeURIComponent(libraryId)}/paths?path=${encodeURIComponent(path)}`,
    {
      method: 'DELETE',
    }
  );
}
```

- [ ] **Step 5: Implement `FolderPickerModal.tsx`**

Create `web/src/components/admin/FolderPickerModal.tsx`:
- Props: `isOpen: boolean`, `initialPath?: string`, `onClose: () => void`, `onSelect: (path: string) => void`.
- Fetches directory contents via `api.browseFilesystem(currentPath)`.
- Renders:
  - Header: "Select Server Folder" with close button.
  - Shortcut pills: e.g. `Root (/)`, `Media`, `Mounts`, `Home`. Clicking fetches that path.
  - Breadcrumb navigation: e.g. `/ > media > movies`. Clicking a breadcrumb segment navigates to that path.
  - Up (`..`) button if `parent_path` is present.
  - Directory items: folder icon, folder name. Double-click or single-click navigates inside.
  - Selection footer: Displays currently active path.
    - CTA button: "Select This Folder" (`onSelect(currentPath)` and `onClose()`).
    - Cancel button (`onClose()`).
- Styled with Tailwind v4 theme tokens (`bg-panel`, `border-border-subtle`, `text-accent`, `bg-cta`, `text-text-main`, `text-muted`).

- [ ] **Step 6: Update `AdminDashboard.tsx`**

- In `handleCreateLibrary`:
  - Validate that `libPaths.length > 0`.
  - Pass `paths: libPaths` in `CreateLibraryPayload`.
- In Add Library modal:
  - Remove manual `<input>` for server path.
  - Render selected folder pills with folder icon and `X` button to remove.
  - Add "+ Add Server Folder" button that opens `FolderPickerModal`.
  - Disallow submitting form when `libPaths.length === 0`.
- In existing library cards:
  - Render all paths in `lib.paths || [lib.path]`.
  - Each path has a remove icon (disabled if only 1 path remains).
  - Add an inline "+ Add Folder" button on the library card opening `FolderPickerModal` to call `api.addLibraryPath(lib.id, selectedPath)`.

- [ ] **Step 7: Run frontend tests & verify build**

Run: `cd web && npm test -- --run`
Run: `cd web && npm run build`
Expected: All tests pass and build outputs cleanly into `web/dist`.

- [ ] **Step 8: Run full workspace checks**

Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 100% passing tests and clean clippy check.

- [ ] **Step 9: Commit changes**

```bash
git add web/
git commit -m "feat(web): add FolderPickerModal and multi-path library management in AdminDashboard"
```

---

## Self-Review Checklist

1. **Spec coverage**:
   - `006_multiple_library_paths.sql` and `LibraryRepo` CRUD -> Task 1
   - Domain model `Library.paths` -> Task 1
   - `GET /api/v1/system/fs` with `RequireAdmin` and shortcuts -> Task 2
   - `POST /api/v1/libraries` multi-path and path add/remove endpoints -> Task 2
   - `kadr-ingest` scanning and watching all paths -> Task 2
   - `FolderPickerModal` interactive navigation & shortcuts -> Task 3
   - `AdminDashboard` replacing manual text path with folder picker + multi-path management -> Task 3
2. **Placeholder scan**: Zero "TBD", "TODO", or vague descriptions.
3. **Type consistency**: `FsBrowseResponse`, `Library.paths`, and API endpoints match exactly across all tasks.
