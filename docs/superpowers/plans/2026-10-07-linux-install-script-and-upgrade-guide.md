# Linux Install Script & Upgrade Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a universal online Linux installer script supporting both fresh installs and zero-downtime upgrades across all non-Debian (and Debian) distros, polish the bundled offline installer in `packaging/scripts/install.sh`, and add complete upgrade instructions to `README.md`.

**Architecture:** Implement a single bash script (`scripts/install.sh` with root symlink `install.sh`) that detects architecture, queries the latest GitHub release or takes an override tag, downloads the pre-compiled musl tarball, handles systemd service pausing/resuming, deploys binaries/web assets, and safely preserves user database and configuration. Update `packaging/scripts/install.sh` with in-place upgrade logic, and document all upgrade procedures in `README.md` and `CHANGELOG.md`.

**Tech Stack:** Bash shell scripting, systemd, GitHub API / Releases, Markdown.

## Global Constraints

- Scripts must be POSIX/Bash compatible (`set -euo pipefail`), with zero external dependencies beyond core utilities (`curl` or `wget`, `tar`, `grep`, `sed`).
- Upgrades must never overwrite or delete existing database (`/var/lib/kadr/kadr.db`) or user configuration files (`/etc/kadr/kadr.toml`, `/etc/kadr/kadr.env`).
- Must support both `x86_64` (amd64) and `aarch64` (arm64 / Raspberry Pi).
- Must run cleanly under non-systemd init systems (print helpful startup instructions instead of failing).
- All workspace Rust tests (`cargo test --workspace`) and web tests (`cd web && npm test -- --run`) must remain 100% passing.

---

### Task 1: Universal Online Linux Installer Script (`scripts/install.sh` & root `install.sh`)

**Files:**
- Create: `scripts/install.sh`
- Create: `install.sh` (symlink to `scripts/install.sh`)
- Test: `scripts/install.sh` (syntax & mock dry-run)

**Interfaces:**
- CLI Usage: `curl -fsSL https://raw.githubusercontent.com/BooDy/kadr/main/scripts/install.sh | sudo bash`
- Optional Env Vars: `KADR_VERSION` (target release tag, default: latest), `KADR_DRY_RUN=1` (test mode).

- [ ] **Step 1: Write `scripts/install.sh`**

Implement `scripts/install.sh`:
1. Check `EUID -eq 0` unless `KADR_DRY_RUN=1`.
2. Detect CPU architecture:
   - `x86_64` / `amd64` $\to$ `x86_64-unknown-linux-musl`
   - `aarch64` / `arm64` $\to$ `aarch64-unknown-linux-musl`
   - Unsupported $\to$ error message and exit 1.
3. Detect downloader (`curl` or `wget`).
4. Resolve target version:
   - Check `KADR_VERSION` environment variable.
   - If unset, query GitHub API: `https://api.github.com/repos/BooDy/kadr/releases/latest` or fallback to `v0.1.1-alpha`.
5. Download release tarball:
   `https://github.com/BooDy/kadr/releases/download/${TAG}/kadr-${TAG}-${TARGET}.tar.gz` to a secure temp directory (`mktemp -d`).
6. Extract tarball and verify contents (`kadr`, `web/`, `kadr.service`).
7. Check if Kadr is already running via systemd:
   - If active, set `WAS_ACTIVE=true` and run `systemctl stop kadr`.
8. Ensure system user `kadr` and group exist.
9. Install executable to `/usr/local/bin/kadr` (`chmod 0755`) and create symlink `/usr/bin/kadr`.
10. Install web assets to `/usr/share/kadr/web/` (`chmod -R a+rX`).
11. Configure `/var/lib/kadr` (`0750 kadr:kadr`) and `/etc/kadr/`.
12. Preserve configuration: if `/etc/kadr/kadr.toml` or `kadr.env` exist, do not overwrite. If missing, copy examples from archive.
13. Systemd setup (if `/etc/systemd/system` and `systemctl` exist):
    - Copy `kadr.service` to `/etc/systemd/system/kadr.service`.
    - Run `systemctl daemon-reload`.
    - If `WAS_ACTIVE=true`, run `systemctl restart kadr`.
    - Else, run `systemctl enable --now kadr`.
14. If non-systemd environment, print explicit manual run instructions.
15. Clean up temporary directory and print success message with Web URL and data paths.

- [ ] **Step 2: Create root symlink `install.sh`**

Run: `ln -s scripts/install.sh install.sh`

- [ ] **Step 3: Syntax and dry-run validation**

Run: `bash -n scripts/install.sh`  
Run: `KADR_DRY_RUN=1 bash scripts/install.sh`  
Expected: Syntax check passes, dry-run detects architecture and version cleanly.

- [ ] **Step 4: Commit**

```bash
git add scripts/install.sh install.sh
git commit -m "feat(packaging): add universal online Linux install and upgrade script"
```

---

### Task 2: Polish Bundled Standalone Tarball Installer (`packaging/scripts/install.sh`)

**Files:**
- Modify: `packaging/scripts/install.sh`

**Interfaces:**
- Executed by users after downloading and extracting `kadr-*.tar.gz`: `sudo ./install.sh`

- [ ] **Step 1: Update `packaging/scripts/install.sh` for in-place upgrades**

Enhance `packaging/scripts/install.sh`:
1. Check if `systemctl` is available and `systemctl is-active --quiet kadr`.
2. If active, print "Stopping active kadr service for upgrade..." and run `systemctl stop kadr`, setting `WAS_ACTIVE=true`.
3. Overwrite `/usr/local/bin/kadr` and update web assets in `/usr/share/kadr/web`.
4. Ensure `/etc/kadr/` files are never overwritten if already present.
5. If `systemd` is present:
   - Install `kadr.service` if not present or updated.
   - Run `systemctl daemon-reload`.
   - If `WAS_ACTIVE=true`, run `systemctl restart kadr`.
   - Else, advise how to start and enable.
6. If not systemd, avoid crashing on missing systemctl.

- [ ] **Step 2: Syntax and verification**

Run: `bash -n packaging/scripts/install.sh`  
Expected: Pass with 0 errors.

- [ ] **Step 3: Commit**

```bash
git add packaging/scripts/install.sh
git commit -m "fix(packaging): improve standalone tarball install.sh for in-place upgrades"
```

---

### Task 3: Upgrade Instructions & Documentation in `README.md` and `CHANGELOG.md`

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Documentation for end-users on upgrading Kadr.

- [ ] **Step 1: Add "Upgrading Kadr" section to `README.md`**

In `README.md`, add:
1. Under **Quick Install for Users (Linux)**:
   - Add **Option 3: Automated One-Line Installer (All Linux Distributions)** showcasing `curl -fsSL https://raw.githubusercontent.com/BooDy/kadr/main/scripts/install.sh | sudo bash`.
2. Add a new top-level section: **## Upgrading Kadr**:
   - **Method A: Automated One-Line Upgrade (All Linux Distros)**
   - **Method B: Debian / Ubuntu (`.deb`) Upgrade**
   - **Method C: Manual Tarball Upgrade**
   - **Data & Configuration Safety Guarantee**: Explains database migration safety and `/var/lib/kadr` persistence.

- [ ] **Step 2: Update `CHANGELOG.md`**

Add entries to `[v0.1.1-alpha]` under `### Added`:
- Universal one-line Linux installer script (`scripts/install.sh`) supporting all major distributions.
- In-place upgrade support in standalone installer.
- Comprehensive upgrade documentation in `README.md`.

- [ ] **Step 3: Verify workspace tests and linter**

Run: `cargo test --workspace`  
Run: `cd web && npm test -- --run`  
Run: `cargo clippy --workspace --all-targets -- -D warnings`

- [ ] **Step 4: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: add comprehensive upgrade guide and online installer instructions to README"
```
