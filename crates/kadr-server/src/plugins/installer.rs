use flate2::read::GzDecoder;
use kadr_core::plugin::PluginManifest;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const MAX_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;

#[cfg(windows)]
pub const BINARY_FILENAME: &str = "plugin.exe";
#[cfg(not(windows))]
pub const BINARY_FILENAME: &str = "plugin";

#[derive(Debug, Clone)]
pub struct PluginPaths {
    pub plugin_dir: PathBuf,
    pub bin_path: PathBuf,
    pub manifest_path: PathBuf,
    pub data_dir: PathBuf,
}

pub fn plugin_paths(plugins_dir: &Path, plugin_id: &str) -> PluginPaths {
    let plugin_dir = plugins_dir.join(plugin_id);
    PluginPaths {
        bin_path: plugin_dir.join("bin").join(BINARY_FILENAME),
        manifest_path: plugin_dir.join("manifest.json"),
        data_dir: plugin_dir.join("data"),
        plugin_dir,
    }
}

pub struct StagedBundle {
    staging_dir: PathBuf,
    bundle_root: PathBuf,
    manifest_src: PathBuf,
    bin_src: PathBuf,
    manifest: PluginManifest,
}

impl Drop for StagedBundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.staging_dir);
    }
}

fn validate_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "archive contains unsafe entry path '{}'",
                    path.display()
                ));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err("archive contains empty entry path".to_string());
    }
    Ok(clean)
}

fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to set executable permissions: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn copy_path_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src)
        .map_err(|e| format!("failed to inspect '{}': {e}", src.display()))?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)
            .map_err(|e| format!("failed to create directory '{}': {e}", dst.display()))?;
        let entries = std::fs::read_dir(src)
            .map_err(|e| format!("failed to read directory '{}': {e}", src.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read directory entry: {e}"))?;
            copy_path_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else if meta.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create parent directory: {e}"))?;
        }
        std::fs::copy(src, dst).map_err(|e| {
            format!(
                "failed to copy '{}' to '{}': {e}",
                src.display(),
                dst.display()
            )
        })?;
    }
    Ok(())
}

fn copy_extra_bundle_assets_to_bin(bundle_root: &Path, bin_dir: &Path) -> Result<(), String> {
    let entries =
        std::fs::read_dir(bundle_root).map_err(|e| format!("failed to read bundle root: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to inspect bundle entry: {e}"))?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if matches!(name_s.as_ref(), "manifest.json" | "data")
            || name_s == BINARY_FILENAME
            || name_s.starts_with('.')
        {
            continue;
        }
        let src_path = entry.path();
        if name_s == "bin" && src_path.is_dir() {
            let sub_entries = std::fs::read_dir(&src_path)
                .map_err(|e| format!("failed to read bundled bin dir: {e}"))?;
            for sub in sub_entries {
                let sub = sub.map_err(|e| format!("failed to inspect bundled bin entry: {e}"))?;
                if sub.file_name().to_string_lossy() == BINARY_FILENAME {
                    continue;
                }
                copy_path_recursive(&sub.path(), &bin_dir.join(sub.file_name()))?;
            }
        } else {
            copy_path_recursive(&src_path, &bin_dir.join(&name))?;
        }
    }
    Ok(())
}

fn sync_extra_bundle_assets_to_plugin_dir(
    bundle_root: &Path,
    plugin_dir: &Path,
) -> Result<(), String> {
    if let Ok(existing) = std::fs::read_dir(plugin_dir) {
        for entry in existing.flatten() {
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if matches!(name_s.as_ref(), "bin" | "data" | "manifest.json")
                || name_s.starts_with('.')
            {
                continue;
            }
            let p = entry.path();
            let res = if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
            res.map_err(|e| format!("failed to clean old plugin asset '{}': {e}", p.display()))?;
        }
    }

    let entries =
        std::fs::read_dir(bundle_root).map_err(|e| format!("failed to read bundle root: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to inspect bundle entry: {e}"))?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if matches!(name_s.as_ref(), "manifest.json" | "bin" | "data")
            || name_s == BINARY_FILENAME
            || name_s.starts_with('.')
        {
            continue;
        }
        copy_path_recursive(&entry.path(), &plugin_dir.join(&name))?;
    }
    Ok(())
}

fn find_binary_in(root: &Path) -> Option<PathBuf> {
    let direct = root.join(BINARY_FILENAME);
    if direct.is_file() {
        return Some(direct);
    }
    let nested = root.join("bin").join(BINARY_FILENAME);
    if nested.is_file() {
        return Some(nested);
    }
    None
}

fn locate_bundle_root(staging_dir: &Path) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let root_manifest = staging_dir.join("manifest.json");
    if root_manifest.is_file() {
        if let Some(root_bin) = find_binary_in(staging_dir) {
            return Ok((staging_dir.to_path_buf(), root_manifest, root_bin));
        }
    }

    let mut subdirs = Vec::new();
    let entries =
        std::fs::read_dir(staging_dir).map_err(|e| format!("failed to read staging dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to inspect staging entry: {e}"))?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            subdirs.push(entry.path());
        }
    }

    if subdirs.len() == 1 {
        let sub_root = subdirs.remove(0);
        let sub_manifest = sub_root.join("manifest.json");
        if sub_manifest.is_file() {
            if let Some(sub_bin) = find_binary_in(&sub_root) {
                return Ok((sub_root, sub_manifest, sub_bin));
            }
        }
    }

    Err(format!(
        "release archive must contain 'manifest.json' and '{BINARY_FILENAME}' at root or in a single top-level directory"
    ))
}

pub fn unpack_bundle(plugins_dir: &Path, archive_bytes: &[u8]) -> Result<StagedBundle, String> {
    std::fs::create_dir_all(plugins_dir)
        .map_err(|e| format!("failed to create plugins directory: {e}"))?;
    let staging_dir = plugins_dir.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("failed to create staging directory: {e}"))?;

    // Guard ensures staging_dir is removed on any error before StagedBundle is constructed.
    struct DirGuard(Option<PathBuf>);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            if let Some(ref p) = self.0 {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
    let mut guard = DirGuard(Some(staging_dir.clone()));

    let gz = GzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(gz);
    let mut total_unpacked: u64 = 0;

    let entries = archive
        .entries()
        .map_err(|e| format!("invalid tar.gz archive: {e}"))?;
    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| format!("corrupt tar entry: {e}"))?;
        let entry_type = entry.header().entry_type();
        if matches!(
            entry_type,
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader
        ) {
            continue;
        }
        let raw_path = entry
            .path()
            .map_err(|e| format!("invalid tar entry path: {e}"))?
            .into_owned();
        if raw_path.as_os_str() == "." || raw_path.as_os_str().is_empty() {
            continue;
        }
        let rel_path = validate_relative_path(&raw_path)?;
        let dest = staging_dir.join(&rel_path);

        match entry_type {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&dest)
                    .map_err(|e| format!("failed to create directory: {e}"))?;
            }
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create parent directory: {e}"))?;
                }
                let remaining = MAX_UNCOMPRESSED_BYTES
                    .checked_sub(total_unpacked)
                    .ok_or_else(|| "uncompressed archive exceeds 250 MiB limit".to_string())?;
                let mut limited = (&mut entry).take(remaining.saturating_add(1));
                let mut out = std::fs::File::create(&dest)
                    .map_err(|e| format!("failed to create extracted file: {e}"))?;
                let copied = std::io::copy(&mut limited, &mut out)
                    .map_err(|e| format!("failed to decompress archive entry: {e}"))?;
                total_unpacked = total_unpacked.saturating_add(copied);
                if total_unpacked > MAX_UNCOMPRESSED_BYTES {
                    return Err("uncompressed archive exceeds 250 MiB limit".to_string());
                }
            }
            other => {
                return Err(format!(
                    "archive contains unsupported entry type '{other:?}' at '{}'",
                    rel_path.display()
                ));
            }
        }
    }

    let (bundle_root, manifest_src, bin_src) = locate_bundle_root(&staging_dir)?;
    let manifest_bytes =
        std::fs::read(&manifest_src).map_err(|e| format!("failed to read manifest.json: {e}"))?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("invalid manifest.json: {e}"))?;
    manifest.validate()?;
    set_executable(&bin_src)?;

    guard.0 = None;
    Ok(StagedBundle {
        staging_dir,
        bundle_root,
        manifest_src,
        bin_src,
        manifest,
    })
}

impl StagedBundle {
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub fn commit_fresh(self, plugins_dir: &Path) -> Result<PluginPaths, String> {
        let paths = plugin_paths(plugins_dir, &self.manifest.id);
        let bin_dir = paths.bin_path.parent().expect("bin_path always has parent");
        std::fs::create_dir_all(bin_dir)
            .map_err(|e| format!("failed to create plugin bin dir: {e}"))?;
        std::fs::create_dir_all(&paths.data_dir)
            .map_err(|e| format!("failed to create plugin data dir: {e}"))?;

        copy_extra_bundle_assets_to_bin(&self.bundle_root, bin_dir)?;
        sync_extra_bundle_assets_to_plugin_dir(&self.bundle_root, &paths.plugin_dir)?;

        std::fs::rename(&self.bin_src, &paths.bin_path)
            .map_err(|e| format!("failed to install plugin binary: {e}"))?;
        std::fs::rename(&self.manifest_src, &paths.manifest_path)
            .map_err(|e| format!("failed to install manifest.json: {e}"))?;
        set_executable(&paths.bin_path)?;
        Ok(paths)
    }

    pub fn commit_update(
        self,
        plugins_dir: &Path,
        expected_id: &str,
    ) -> Result<PluginPaths, String> {
        if self.manifest.id != expected_id {
            return Err(format!(
                "updated manifest id '{}' does not match installed plugin '{}'",
                self.manifest.id, expected_id
            ));
        }
        let paths = plugin_paths(plugins_dir, expected_id);
        let bin_dir = paths.bin_path.parent().expect("bin_path always has parent");
        std::fs::create_dir_all(&paths.plugin_dir)
            .map_err(|e| format!("failed to create plugin dir: {e}"))?;
        std::fs::create_dir_all(&paths.data_dir)
            .map_err(|e| format!("failed to create plugin data dir: {e}"))?;

        let tmp_bin_dir = paths.plugin_dir.join(".bin.new");
        if tmp_bin_dir.exists() {
            let _ = std::fs::remove_dir_all(&tmp_bin_dir);
        }
        std::fs::create_dir_all(&tmp_bin_dir)
            .map_err(|e| format!("failed to create staged bin dir: {e}"))?;
        let tmp_bin = tmp_bin_dir.join(BINARY_FILENAME);
        std::fs::copy(&self.bin_src, &tmp_bin)
            .map_err(|e| format!("failed to stage updated binary: {e}"))?;
        set_executable(&tmp_bin)?;
        copy_extra_bundle_assets_to_bin(&self.bundle_root, &tmp_bin_dir)?;

        let tmp_manifest = paths.plugin_dir.join(".manifest.json.new");
        std::fs::copy(&self.manifest_src, &tmp_manifest)
            .map_err(|e| format!("failed to stage updated manifest: {e}"))?;

        if bin_dir.exists() {
            std::fs::remove_dir_all(bin_dir)
                .map_err(|e| format!("failed to remove previous bin dir: {e}"))?;
        }
        std::fs::rename(&tmp_bin_dir, bin_dir)
            .map_err(|e| format!("failed to swap updated bin dir: {e}"))?;
        std::fs::rename(&tmp_manifest, &paths.manifest_path)
            .map_err(|e| format!("failed to swap updated manifest: {e}"))?;
        sync_extra_bundle_assets_to_plugin_dir(&self.bundle_root, &paths.plugin_dir)?;
        Ok(paths)
    }
}

pub fn remove_plugin_files(
    plugins_dir: &Path,
    plugin_id: &str,
    purge: bool,
) -> std::io::Result<()> {
    let paths = plugin_paths(plugins_dir, plugin_id);
    if purge {
        if paths.plugin_dir.exists() {
            std::fs::remove_dir_all(&paths.plugin_dir)?;
        }
    } else if paths.plugin_dir.exists() {
        for entry in std::fs::read_dir(&paths.plugin_dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy() == "data" {
                continue;
            }
            let p = entry.path();
            if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(p)?;
            } else {
                std::fs::remove_file(p)?;
            }
        }
    }
    Ok(())
}
