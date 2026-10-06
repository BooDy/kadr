# Server Name & Network Autodiscovery API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a configurable server name and an unauthenticated network autodiscovery endpoint (`GET /api/v1/discovery`) returning persistent server ID, display name, version, and setup status to allow LAN clients to locate and connect to Kadr.

**Architecture:** A persistent UUID generated on startup in `<data_dir>/server_id` (`ServerIdentity`), an updated `ServerSettings` in `kadr.toml` exposing `name`, an unauthenticated Axum route `GET /api/v1/discovery`, and a TV-friendly "Server Name" input field in the Web Admin Dashboard.

**Tech Stack:** Rust (Axum, tokio, uuid, serde, toml), React 19, TypeScript, Tailwind CSS, Vitest.

## Global Constraints
- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- Public discovery route must NOT expose private paths, file systems, or secret tokens.
- Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `ring-highlight`, `border-border-subtle`).
- 10-foot TV UI focus rings: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all interactive inputs.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.

---

### Task 1: Server Identity & Configuration Updates (`crates/kadr-server`)

**Files:**
* Create: `crates/kadr-server/src/identity.rs`
* Modify: `crates/kadr-server/src/lib.rs`
* Modify: `crates/kadr-server/src/config.rs:5-16, 50-60`
* Modify: `crates/kadr-server/src/api/config_routes.rs:12-75`
* Test: `crates/kadr-server/tests/identity_test.rs`
* Test: `crates/kadr-server/tests/config_test.rs`

**Interfaces:**
* Produces:
  ```rust
  #[derive(Debug, Clone)]
  pub struct ServerIdentity {
      pub id: String,
  }

  pub fn load_or_create_server_id(data_dir: &std::path::Path) -> Result<String, std::io::Error>;

  pub struct ServerSettings {
      pub name: String,
      // ... host, port, data_dir, web_dir
  }
  ```

- [ ] **Step 1: Write failing unit tests in `crates/kadr-server/tests/identity_test.rs`**

Test scenarios:
1. `load_or_create_server_id` when `<data_dir>/server_id` does not exist:
   - Creates the file containing a valid UUID v4 string.
   - Returns the generated UUID string.
2. `load_or_create_server_id` when `<data_dir>/server_id` already exists:
   - Reads the existing UUID without changing or overwriting it.
3. Test `ServerSettings` default name is `"Kadr Media Server"`.
4. Test deserialization of `[server] name = "Living Room"` in `config_test.rs`.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_server --test identity_test`
Expected: FAIL (modules/functions do not exist yet).

- [ ] **Step 3: Implement `identity.rs` and update `config.rs` & `config_routes.rs`**

1. Create `crates/kadr-server/src/identity.rs`:
   - Implement `ServerIdentity` struct and `load_or_create_server_id(data_dir: &Path) -> Result<String, std::io::Error>`.
   - Export in `crates/kadr-server/src/lib.rs`.
2. Update `crates/kadr-server/src/config.rs`:
   - Add `pub name: String` to `ServerSettings` with `#[serde(default = "default_server_name")]`.
   - Add `fn default_server_name() -> String { "Kadr Media Server".to_string() }`.
3. Update `crates/kadr-server/src/api/config_routes.rs`:
   - Add `pub name: String` to `SystemConfigResponse`.
   - Add `pub name: Option<String>` to `UpdateConfigPayload`.
   - In `get_config`, return `name: cfg.server.name.clone()`.
   - In `update_config`, if `payload.name` is present and non-empty after trimming, set `cfg.server.name = trimmed`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test -p kadr_server --test identity_test --test config_test`
Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/identity.rs crates/kadr-server/src/lib.rs crates/kadr-server/src/config.rs crates/kadr-server/src/api/config_routes.rs crates/kadr-server/tests/identity_test.rs crates/kadr-server/tests/config_test.rs
git commit -m "feat(server): add persistent ServerIdentity and server name configuration"
```

---

### Task 2: Network Autodiscovery API Endpoint (`crates/kadr-server`)

**Files:**
* Create: `crates/kadr-server/src/api/discovery_routes.rs`
* Modify: `crates/kadr-server/src/api/mod.rs`
* Modify: `crates/kadr-server/src/main.rs:160-200`
* Test: `crates/kadr-server/tests/discovery_test.rs`

**Interfaces:**
* Produces:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
  pub struct DiscoveryResponse {
      pub app: String,               // "kadr"
      pub server_id: String,         // persistent UUID
      pub name: String,              // configured server name
      pub version: String,           // env!("CARGO_PKG_VERSION")
      pub protocol_version: u32,     // 1
      pub port: u16,                 // configured port
      pub setup_completed: bool,     // user_repo.count() > 0
      pub status: String,            // "online"
  }

  pub async fn get_discovery(
      Extension(identity): Extension<Arc<ServerIdentity>>,
      Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
      Extension(user_repo): Extension<UserRepository>,
  ) -> (StatusCode, Json<DiscoveryResponse>);
  ```

- [ ] **Step 1: Write failing integration tests in `crates/kadr-server/tests/discovery_test.rs`**

Test scenarios:
1. `GET /api/v1/discovery` without authentication:
   - Returns 200 OK with `Content-Type: application/json`.
   - `app == "kadr"`.
   - `server_id` matches the configured `ServerIdentity`.
   - `name` matches `config.server.name`.
   - `version == env!("CARGO_PKG_VERSION")`.
   - `protocol_version == 1`.
   - `port` matches `config.server.port`.
   - `status == "online"`.
   - `setup_completed == false` when database has zero users.
2. `setup_completed == true` after an admin user is inserted.
3. Updating server name via `PUT /api/v1/system/config` updates subsequent responses from `GET /api/v1/discovery`.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_server --test discovery_test`
Expected: FAIL.

- [ ] **Step 3: Implement `discovery_routes.rs` and router registration**

1. Create `crates/kadr-server/src/api/discovery_routes.rs`:
   - Implement `DiscoveryResponse` and `get_discovery` handler.
2. In `crates/kadr-server/src/api/mod.rs`:
   - Register `.route("/api/v1/discovery", get(discovery_routes::get_discovery))`.
   - Add `identity: Arc<ServerIdentity>` to `create_router_with_ingest` / router builders and layer `.layer(Extension(identity))`.
   - Provide default `ServerIdentity` (`"00000000-0000-0000-0000-000000000000"`) for lightweight test constructors if identity is not supplied.
3. In `crates/kadr-server/src/main.rs`:
   - Call `load_or_create_server_id(&config.server.data_dir)` on startup.
   - Pass `Arc::new(ServerIdentity { id: server_id })` to router.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test -p kadr_server --test discovery_test`
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/api/discovery_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/src/main.rs crates/kadr-server/tests/discovery_test.rs
git commit -m "feat(server): add unauthenticated network autodiscovery endpoint GET /api/v1/discovery"
```

---

### Task 3: Web Client Integration & Admin Dashboard UI (`web/`)

**Files:**
* Modify: `web/src/types/index.ts`
* Modify: `web/src/api/client.ts:310-340`
* Modify: `web/src/components/admin/AdminDashboard.tsx:65-95, 450-510`
* Test: `web/src/components/admin/AdminDashboard.test.tsx`

**Interfaces:**
* Produces:
  ```typescript
  export interface DiscoveryResponse {
    app: 'kadr';
    server_id: string;
    name: string;
    version: string;
    protocol_version: number;
    port: number;
    setup_completed: boolean;
    status: 'online';
  }

  // in client.ts
  getDiscoveryInfo(): Promise<DiscoveryResponse>;
  ```

- [ ] **Step 1: Write failing component tests in `web/src/components/admin/AdminDashboard.test.tsx`**

Test scenarios:
1. Verify "Server Name" input displays the current server name from `getSystemConfig`.
2. Verify editing Server Name and clicking "Save Changes" invokes `api.updateSystemConfig` with `{ name: 'New Server Name', ... }`.
3. Verify client test for `api.getDiscoveryInfo()`.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run AdminDashboard.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement web client methods and Admin Dashboard UI**

1. Update `web/src/types/index.ts`:
   - Add `DiscoveryResponse`.
   - Add `name: string` to `SystemConfig`.
   - Add `name?: string` to `UpdateConfigPayload`.
2. Update `web/src/api/client.ts`:
   - Implement `getDiscoveryInfo(): Promise<DiscoveryResponse>`.
3. In `web/src/components/admin/AdminDashboard.tsx`:
   - Add `const [editName, setEditName] = useState('Kadr Media Server');`
   - In `loadData()`: set `setEditName(cfg.name || 'Kadr Media Server')`.
   - In "Server Configuration" form:
     - Add Server Name input field with label and placeholder:
       ```tsx
       <div>
         <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-2">
           Server Name
         </label>
         <input
           type="text"
           value={editName}
           onChange={(e) => setEditName(e.target.value)}
           placeholder="e.g. Living Room Kadr"
           className="w-full bg-canvas border border-border-subtle rounded-lg px-3 py-2 text-text-main text-sm focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
         />
       </div>
       ```
     - Include `name: editName.trim() || undefined` in `handleSaveConfig`.

- [ ] **Step 4: Run tests and production build**

Run: `cd web && npm test -- --run`
Run: `npm run build`
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/types/index.ts web/src/api/client.ts web/src/components/admin/AdminDashboard.tsx web/src/components/admin/AdminDashboard.test.tsx
git commit -m "feat(web): add Server Name configuration in AdminDashboard and DiscoveryInfo client API"
```
