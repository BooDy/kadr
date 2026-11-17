use flate2::{write::GzEncoder, Compression};
use kadr_server::plugins::installer::{remove_plugin_files, unpack_bundle};
use serde_json::json;

fn make_tar_gz(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut gz);
        for (path, content, kind) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_mode(0o644);
            if *kind == tar::EntryType::Symlink {
                header.set_size(0);
                builder
                    .append_link(&mut header, *path, "/etc/passwd")
                    .unwrap();
            } else {
                header.set_size(content.len() as u64);
                header.set_cksum();
                builder.append_data(&mut header, *path, *content).unwrap();
            }
        }
        builder.finish().unwrap();
    }
    gz.finish().unwrap()
}

fn manifest_bytes(id: &str, version: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "id": id,
        "name": format!("Plugin {id}"),
        "version": version,
        "api_version": 1,
        "permissions": ["items:publish"],
        "capabilities": { "virtual_library": true }
    }))
    .unwrap()
}

#[test]
fn unpacks_root_and_single_prefix_layouts_preserves_static_assets_and_data() {
    let dir = tempfile::tempdir().unwrap();
    let m1 = manifest_bytes("hello", "0.1.0");
    let flat = make_tar_gz(&[
        ("manifest.json", &m1, tar::EntryType::Regular),
        ("plugin", b"#!/bin/sh\necho v1\n", tar::EntryType::Regular),
        (
            "web/dist/index.html",
            b"<html>v1</html>",
            tar::EntryType::Regular,
        ),
    ]);

    let staged = unpack_bundle(dir.path(), &flat).unwrap();
    assert_eq!(staged.manifest().id, "hello");
    let paths = staged.commit_fresh(dir.path()).unwrap();
    assert!(paths.bin_path.exists());
    assert!(paths.manifest_path.exists());
    assert!(paths.data_dir.exists());
    // Bundled static UI assets available both under <plugin_dir>/bin/web/dist and <plugin_dir>/web/dist
    assert_eq!(
        std::fs::read(paths.plugin_dir.join("bin/web/dist/index.html")).unwrap(),
        b"<html>v1</html>"
    );
    assert_eq!(
        std::fs::read(paths.plugin_dir.join("web/dist/index.html")).unwrap(),
        b"<html>v1</html>"
    );

    // Write plugin runtime state in data_dir — update and non-purge remove must preserve it.
    let state_file = paths.data_dir.join("state.txt");
    std::fs::write(&state_file, b"keep-me").unwrap();

    // Update using a nested top-level directory layout (`hello-0.2.0/...`) with updated web assets
    let m2 = manifest_bytes("hello", "0.2.0");
    let nested = make_tar_gz(&[
        ("hello-0.2.0/manifest.json", &m2, tar::EntryType::Regular),
        (
            "hello-0.2.0/plugin",
            b"#!/bin/sh\necho v2\n",
            tar::EntryType::Regular,
        ),
        (
            "hello-0.2.0/web/dist/app.js",
            b"console.log('v2');",
            tar::EntryType::Regular,
        ),
    ]);
    let staged_v2 = unpack_bundle(dir.path(), &nested).unwrap();
    assert_eq!(staged_v2.manifest().version, "0.2.0");
    let updated_paths = staged_v2.commit_update(dir.path(), "hello").unwrap();
    assert_eq!(
        std::fs::read(&updated_paths.bin_path).unwrap(),
        b"#!/bin/sh\necho v2\n"
    );
    assert_eq!(std::fs::read(&state_file).unwrap(), b"keep-me");
    assert!(!updated_paths
        .plugin_dir
        .join("bin/web/dist/index.html")
        .exists());
    assert_eq!(
        std::fs::read(updated_paths.plugin_dir.join("bin/web/dist/app.js")).unwrap(),
        b"console.log('v2');"
    );
    assert_eq!(
        std::fs::read(updated_paths.plugin_dir.join("web/dist/app.js")).unwrap(),
        b"console.log('v2');"
    );

    // Reject manifest id mismatch on update and verify staging directory is cleaned up.
    let wrong_id = make_tar_gz(&[
        (
            "manifest.json",
            &manifest_bytes("other", "0.3.0"),
            tar::EntryType::Regular,
        ),
        ("plugin", b"bin", tar::EntryType::Regular),
    ]);
    let staged_bad = unpack_bundle(dir.path(), &wrong_id).unwrap();
    assert!(staged_bad.commit_update(dir.path(), "hello").is_err());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".staging-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "staging directories must be cleaned up"
    );

    // Non-purge remove deletes bin + manifest + extra assets but preserves data/state.txt
    remove_plugin_files(dir.path(), "hello", false).unwrap();
    assert!(!updated_paths.bin_path.exists());
    assert!(!updated_paths.manifest_path.exists());
    assert!(!updated_paths.plugin_dir.join("web").exists());
    assert!(state_file.exists());

    // Purge remove deletes everything
    remove_plugin_files(dir.path(), "hello", true).unwrap();
    assert!(!updated_paths.plugin_dir.exists());
}

#[test]
fn rejects_symlinks_missing_files_and_invalid_manifests_without_leaving_staging_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest_bytes("hello", "0.1.0");

    let with_symlink = make_tar_gz(&[
        ("manifest.json", &m, tar::EntryType::Regular),
        ("plugin", b"bin", tar::EntryType::Regular),
        ("escape", b"", tar::EntryType::Symlink),
    ]);
    assert!(unpack_bundle(dir.path(), &with_symlink).is_err());

    let missing_bin = make_tar_gz(&[("manifest.json", &m, tar::EntryType::Regular)]);
    assert!(unpack_bundle(dir.path(), &missing_bin).is_err());

    let reserved_manifest = serde_json::to_vec(&json!({
        "id": "install",
        "name": "Bad",
        "version": "0.1.0",
        "api_version": 1
    }))
    .unwrap();
    let reserved = make_tar_gz(&[
        ("manifest.json", &reserved_manifest, tar::EntryType::Regular),
        ("plugin", b"bin", tar::EntryType::Regular),
    ]);
    assert!(unpack_bundle(dir.path(), &reserved).is_err());

    // Hardlink, FIFO, and device node rejection
    for bad_kind in [
        tar::EntryType::Link,
        tar::EntryType::Fifo,
        tar::EntryType::Char,
    ] {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gz);
            for (name, body) in [("manifest.json", m.as_slice()), ("plugin", b"bin")] {
                let mut h = tar::Header::new_gnu();
                h.set_entry_type(tar::EntryType::Regular);
                h.set_mode(0o644);
                h.set_size(body.len() as u64);
                h.set_cksum();
                builder.append_data(&mut h, name, body).unwrap();
            }
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(bad_kind);
            h.set_mode(0o644);
            h.set_size(0);
            if bad_kind == tar::EntryType::Link {
                builder.append_link(&mut h, "hardlink", "plugin").unwrap();
            } else {
                h.set_cksum();
                builder
                    .append_data(&mut h, "special", std::io::empty())
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        assert!(unpack_bundle(dir.path(), &gz.finish().unwrap()).is_err());
    }

    // Raw '..' traversal and absolute path rejection via crafted tar header bytes
    for raw_bad_path in [b"../escape\0".as_slice(), b"/etc/escape\0".as_slice()] {
        let mut raw_tar = vec![0u8; 1024];
        raw_tar[..raw_bad_path.len()].copy_from_slice(raw_bad_path);
        raw_tar[100..108].copy_from_slice(b"0000644\0");
        raw_tar[108..116].copy_from_slice(b"0000000\0");
        raw_tar[116..124].copy_from_slice(b"0000000\0");
        raw_tar[124..136].copy_from_slice(b"00000000000\0");
        raw_tar[136..148].copy_from_slice(b"00000000000\0");
        raw_tar[148..156].fill(b' ');
        raw_tar[156] = b'0';
        let cksum: u32 = raw_tar[..512].iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{cksum:06o}\0 ");
        raw_tar[148..156].copy_from_slice(cksum_str.as_bytes());

        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut gz, &raw_tar).unwrap();
        assert!(unpack_bundle(dir.path(), &gz.finish().unwrap()).is_err());
    }

    let count = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count, 0, "failed unpacks must leave zero staging dirs");
}
