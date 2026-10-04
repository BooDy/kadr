# Private Library with PIN Protection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the "Private Library" feature across Kadr, enabling users to mark any media library as private with a 4-digit PIN, isolating its media from global views and stream endpoints while locked, and unlocking via the Web UI PIN keypad during an active session.

**Architecture:** SQLite database migration adds `is_private` and `pin_hash` to `libraries`. The storage layer filters out private items unless their library IDs are explicitly provided in the caller's unlocked set. The Axum server provides a rate-limited PIN verification endpoint returning signed unlock tokens, enforced at route and streaming levels via `X-Kadr-Unlocked` header extraction. The React Web UI provides private library creation in Admin Dashboard, lock badges in navigation, and session-based unlocking via `PinKeypad`.

**Tech Stack:** Rust (Axum, SQLx, SQLite, Argon2, HMAC-SHA256, Tokio), TypeScript, React 19, Tailwind CSS, Vitest.

## Global Constraints
- Pure-Rust baseline with zero native external C runtime dependencies (musl compatible).
- SQLite operations must maintain WAL mode, PRAGMA synchronous = NORMAL, PRAGMA foreign_keys = ON, PRAGMA busy_timeout = 5000.
- PINs must be strictly validated as 4 numeric digits (`^\d{4}$`) and hashed with Argon2id.
- Private media stream chunks and metadata must return `403 Forbidden` (`{"error": "LIBRARY_LOCKED"}`) when unauthorized.
- Frontend must persist unlocked tokens in `sessionStorage` under `kadr_unlocked_libraries` and clear them on logout.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.

---

### Task 1: Core Models & Storage Migration for Private Libraries (`kadr-core` & `kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/migrations/005_private_libraries.sql`
- Modify: `crates/kadr-core/src/models.rs:19-27`
- Modify: `crates/kadr-storage/src/repos/library_repo.rs:1-120`
- Modify: `crates/kadr-storage/src/migrations.rs:1-50`
- Test: `crates/kadr-storage/tests/private_libraries_storage_test.rs`

**Interfaces:**
- Consumes: `kadr_core::models::{Library, MediaType}`
- Produces:
  ```rust
  pub struct Library {
      pub id: String,
      pub name: String,
      pub path: PathBuf,
      pub media_type: MediaType,
      pub is_private: bool,
      pub pin_hash: Option<String>,
      pub created_at: i64,
  }
  ```
  `LibraryRepo::create(&self, id, name, path, media_type, is_private, pin_hash)`
  `LibraryRepo::update_privacy(&self, id, is_private, pin_hash)`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-storage/tests/private_libraries_storage_test.rs`:
```rust
use kadr_core::models::MediaType;
use kadr_storage::pool::initialize_database;
use kadr_storage::repos::LibraryRepo;

#[tokio::test]
async fn test_private_library_storage_and_migration() {
    let pool = initialize_database(":memory:", 1).await.expect("db init failed");
    let repo = LibraryRepo::new(pool.clone());

    // Create public library
    let public_lib = repo.create("lib-pub", "Public Movies", "/media/pub", MediaType::Movie, false, None)
        .await
        .expect("create public failed");
    assert!(!public_lib.is_private);
    assert_eq!(public_lib.pin_hash, None);

    // Create private library
    let private_lib = repo.create("lib-priv", "Secret Vault", "/media/secret", MediaType::Movie, true, Some("hash123"))
        .await
        .expect("create private failed");
    assert!(private_lib.is_private);
    assert_eq!(private_lib.pin_hash.as_deref(), Some("hash123"));

    // Fetch by id
    let fetched = repo.get_by_id("lib-priv").await.expect("fetch failed").expect("missing lib");
    assert!(fetched.is_private);
    assert_eq!(fetched.pin_hash.as_deref(), Some("hash123"));
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-storage --test private_libraries_storage_test`
Expected: FAIL due to missing `is_private` fields in `Library` and `LibraryRepo`.

- [ ] **Step 3: Implement migration and repo updates**
1. Add `crates/kadr-storage/src/migrations/005_private_libraries.sql`:
```sql
ALTER TABLE libraries ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0;
ALTER TABLE libraries ADD COLUMN pin_hash TEXT DEFAULT NULL;
CREATE INDEX IF NOT EXISTS idx_libraries_private ON libraries(is_private);
```
2. Update `crates/kadr-core/src/models.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub media_type: MediaType,
    pub is_private: bool,
    #[serde(skip_serializing)]
    pub pin_hash: Option<String>,
    pub created_at: i64,
}
```
3. Update `crates/kadr-storage/src/repos/library_repo.rs` to persist and read `is_private` and `pin_hash`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-storage --test private_libraries_storage_test`
Expected: PASS

- [ ] **Step 5: Run existing storage tests to verify no regressions**
Run: `cargo test -p kadr-storage`
Expected: PASS

- [ ] **Step 6: Commit**
```bash
git add crates/kadr-core/src/models.rs crates/kadr-storage/
git commit -m "feat(storage): add private library schema migration and core model fields"
```

---

### Task 2: Storage Filtering & Media Isolation for Private Libraries (`kadr-storage`)

**Files:**
- Modify: `crates/kadr-storage/src/repos/media_item_repo.rs:1-250`
- Modify: `crates/kadr-storage/src/repos/widget_queries.rs:1-350`
- Test: `crates/kadr-storage/tests/private_media_filtering_test.rs`

**Interfaces:**
- Consumes: `unlocked_library_ids: &[String]`
- Produces:
  `MediaItemRepo::find_by_library_paginated(&self, library_id, page, page_size, unlocked_ids)`
  `WidgetQueries::find_top_rated(&self, limit, unlocked_ids)`
  `WidgetQueries::find_recently_added(&self, limit, unlocked_ids)`
  `WidgetQueries::find_spotlight_candidate(&self, unlocked_ids)`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-storage/tests/private_media_filtering_test.rs`:
```rust
use kadr_core::models::{MediaItem, MediaType};
use kadr_storage::pool::initialize_database;
use kadr_storage::repos::{LibraryRepo, MediaItemRepo};

#[tokio::test]
async fn test_private_library_media_filtering() {
    let pool = initialize_database(":memory:", 1).await.expect("db init failed");
    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());

    lib_repo.create("pub-1", "Public", "/media/pub", MediaType::Movie, false, None).await.unwrap();
    lib_repo.create("priv-1", "Private", "/media/priv", MediaType::Movie, true, Some("pin_hash")).await.unwrap();

    let pub_item = MediaItem {
        id: 1,
        library_id: "pub-1".into(),
        item_type: MediaType::Movie,
        title: "Public Movie".into(),
        file_path: "/media/pub/m.mp4".into(),
        file_name: "m.mp4".into(),
        ..Default::default()
    };
    let priv_item = MediaItem {
        id: 2,
        library_id: "priv-1".into(),
        item_type: MediaType::Movie,
        title: "Private Video".into(),
        file_path: "/media/priv/v.mp4".into(),
        file_name: "v.mp4".into(),
        ..Default::default()
    };
    item_repo.batch_upsert(&[pub_item, priv_item]).await.unwrap();

    // Query with empty unlocked list -> only public item returned
    let locked_results = item_repo.find_recently_added(10, &[]).await.unwrap();
    assert_eq!(locked_results.len(), 1);
    assert_eq!(locked_results[0].title, "Public Movie");

    // Query with "priv-1" unlocked -> both returned
    let unlocked_results = item_repo.find_recently_added(10, &["priv-1".to_string()]).await.unwrap();
    assert_eq!(unlocked_results.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-storage --test private_media_filtering_test`
Expected: FAIL due to missing parameter / filtering logic.

- [ ] **Step 3: Implement privacy filtering in SQL queries**
Update `media_item_repo.rs` and `widget_queries.rs`:
Incorporate `JOIN libraries l ON media_items.library_id = l.id` and add condition:
```sql
WHERE (l.is_private = 0 OR l.id IN (...unlocked_ids...))
```

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-storage --test private_media_filtering_test`
Expected: PASS

- [ ] **Step 5: Run existing storage tests to verify no regressions**
Run: `cargo test -p kadr-storage`
Expected: PASS

- [ ] **Step 6: Commit**
```bash
git add crates/kadr-storage/
git commit -m "feat(storage): isolate private library items from widget and media queries unless unlocked"
```

---

### Task 3: Backend REST API for Unlocking & PIN Management (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/unlock_token.rs`
- Modify: `crates/kadr-server/src/api/library_routes.rs:1-250`
- Modify: `crates/kadr-server/src/api/mod.rs:1-120`
- Test: `crates/kadr-server/tests/private_library_routes_test.rs`

**Interfaces:**
- Consumes: `LibraryRepo`, `argon2`, `hmac`, `sha2`
- Produces:
  `POST /api/v1/libraries/{id}/unlock` -> `{ library_id, token, expires_at }`
  `POST /api/v1/libraries` with `{ is_private: bool, pin: Option<String> }`
  `UnlockedLibraries` Axum extractor reading `X-Kadr-Unlocked` header

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/private_library_routes_test.rs`:
```rust
use axum::http::{Request, StatusCode};
use axum::body::Body;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_create_and_unlock_private_library() {
    let (app, admin_token) = setup_test_app().await;

    // 1. Create private library with PIN 4321
    let create_payload = serde_json::json!({
        "name": "Personal Home Videos",
        "path": "/tmp/kadr_test_private_lib",
        "media_type": "Movie",
        "is_private": true,
        "pin": "4321"
    });
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let lib_id = created["id"].as_str().unwrap();
    assert_eq!(created["is_private"], true);

    // 2. Unlock with wrong PIN -> 401 Unauthorized
    let wrong_req = Request::post(format!("/api/v1/libraries/{}/unlock", lib_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "0000"}).to_string()))
        .unwrap();
    let wrong_resp = app.clone().oneshot(wrong_req).await.unwrap();
    assert_eq!(wrong_resp.status(), StatusCode::UNAUTHORIZED);

    // 3. Unlock with correct PIN -> 200 OK + token
    let right_req = Request::post(format!("/api/v1/libraries/{}/unlock", lib_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "4321"}).to_string()))
        .unwrap();
    let right_resp = app.clone().oneshot(right_req).await.unwrap();
    assert_eq!(right_resp.status(), StatusCode::OK);
    let unlock_body: serde_json::Value = serde_json::from_slice(
        &right_resp.into_body().collect().await.unwrap().to_bytes()
    ).unwrap();
    assert!(unlock_body["token"].is_string());
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test private_library_routes_test`
Expected: FAIL

- [ ] **Step 3: Implement unlock token generator and endpoints**
1. Implement `crates/kadr-server/src/api/unlock_token.rs` using HMAC-SHA256 to sign and verify `{ library_id, exp }`.
2. Implement Axum extractor `UnlockedLibraries` that reads `X-Kadr-Unlocked: <token1>,<token2>` and verifies valid tokens.
3. Add `POST /api/v1/libraries/{id}/unlock` route handler verifying Argon2id hash.
4. Update `POST /api/v1/libraries` to validate 4-digit PIN when `is_private == true`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test private_library_routes_test`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add crates/kadr-server/
git commit -m "feat(server): add private library unlock route, HMAC token generator, and extractor"
```

---

### Task 4: Media Streaming & Direct Item Authorization (`kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/api/streaming.rs:1-180`
- Modify: `crates/kadr-server/src/api/screen_routes.rs:1-250`
- Test: `crates/kadr-server/tests/private_streaming_auth_test.rs`

**Interfaces:**
- Consumes: `UnlockedLibraries` extractor
- Produces:
  `GET /api/v1/stream/{item_id}` returns `403 Forbidden` if parent library is private and not in `unlocked_ids`.
  `GET /api/v1/screens/{screen_id}` forwards `unlocked_ids` to widget resolver.

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/private_streaming_auth_test.rs`:
```rust
#[tokio::test]
async fn test_streaming_private_item_requires_unlock_token() {
    let (app, admin_token, private_item_id, unlock_token) = setup_app_with_private_item().await;

    // 1. Stream without unlock token -> 403 Forbidden
    let req = Request::get(format!("/api/v1/stream/{}", private_item_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 2. Stream with unlock token -> 206 Partial Content (or 200 OK)
    let req_with_token = Request::get(format!("/api/v1/stream/{}", private_item_id))
        .header("X-Kadr-Unlocked", &unlock_token)
        .header("Range", "bytes=0-100")
        .body(Body::empty())
        .unwrap();
    let resp_ok = app.clone().oneshot(req_with_token).await.unwrap();
    assert_eq!(resp_ok.status(), StatusCode::PARTIAL_CONTENT);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test private_streaming_auth_test`
Expected: FAIL

- [ ] **Step 3: Implement stream and screen authorization checks**
1. In `streaming.rs`, fetch item and join library. If `library.is_private && !unlocked_ids.contains(&item.library_id)`, return `(StatusCode::FORBIDDEN, Json(json!({"error": "LIBRARY_LOCKED"})))`.
2. In `screen_routes.rs`, pass `unlocked_ids` into `WidgetResolver`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test private_streaming_auth_test`
Expected: PASS

- [ ] **Step 5: Run all workspace tests to verify zero regressions**
Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 6: Commit**
```bash
git add crates/kadr-server/
git commit -m "feat(server): guard video streaming and screen layouts behind private library unlock tokens"
```

---

### Task 5: Web UI Frontend Integration (`web/`)

**Files:**
- Modify: `web/src/types/index.ts:1-120`
- Modify: `web/src/api/client.ts:1-250`
- Modify: `web/src/components/admin/AdminDashboard.tsx:1-665`
- Modify: `web/src/App.tsx:1-400`
- Test: `web/src/components/admin/AdminDashboard.test.tsx`
- Test: `web/src/App.test.tsx`

**Interfaces:**
- Consumes: `api.unlockLibrary(id, pin)`, `api.createLibrary({ ...payload, is_private, pin })`
- Produces:
  `kadr_unlocked_libraries` in `sessionStorage`
  Automatic attachment of `X-Kadr-Unlocked` header in `client.ts`
  Add Library modal with "Private Library" switch and PIN input
  Lock/Unlock badge rendering and `PinKeypad` unlock modal

- [ ] **Step 1: Write failing component tests**
In `web/src/components/admin/AdminDashboard.test.tsx`:
Add test verifying that toggling "Private Library" shows the 4-digit PIN input, and submitting creates the library with `is_private: true` and `pin: "5678"`.
In `web/src/App.test.tsx`:
Add test verifying that clicking a locked private library triggers the PIN keypad prompt.

- [ ] **Step 2: Run tests to verify failure**
Run: `cd web && npm test -- --run AdminDashboard.test.tsx App.test.tsx`
Expected: FAIL

- [ ] **Step 3: Implement Web UI components and client updates**
1. In `web/src/types/index.ts`:
   Add `is_private: boolean` to `Library`.
   Update `CreateLibraryPayload` to include `is_private?: boolean; pin?: string;`.
   Add `UnlockLibraryResponse { library_id: string; token: string; expires_at: number; }`.
2. In `web/src/api/client.ts`:
   Implement `unlockLibrary(id: string, pin: string)`.
   Manage `sessionStorage.getItem('kadr_unlocked_libraries')` and inject `X-Kadr-Unlocked` header into all requests.
3. In `AdminDashboard.tsx`:
   Add "Private Library" toggle and 4-digit PIN field with digit masking in "+ Add Library" modal.
   Render yellow `Lock` badge on private library cards.
4. In `App.tsx` and Navigation:
   Display `Lock` icon on private library tabs.
   Prompt with `PinKeypad` when clicking a locked library.
   On unlock success, update session state and display library contents.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cd web && npm test -- --run`
Expected: 100% PASS

- [ ] **Step 5: Verify web build**
Run: `cd web && npm run build`
Expected: Clean build in `web/dist`

- [ ] **Step 6: Run full workspace tests and clippy**
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 100% PASS and zero warnings

- [ ] **Step 7: Commit**
```bash
git add web/
git commit -m "feat(web): add private library toggle, PIN setup in admin, and session-based unlock UI"
```
