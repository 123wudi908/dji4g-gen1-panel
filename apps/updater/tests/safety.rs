use dji4g_updater::{
    UpdateError, archive::stage_archive, install::replace_and_confirm, verify_archive,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};
use zip::{ZipWriter, write::SimpleFileOptions};

const NAMES: [&str; 8] = [
    "dji4g-panel.exe",
    "dji4g-helper.exe",
    "dji4g-updater.exe",
    "使用说明.txt",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "THIRD-PARTY-NOTICES.txt",
    "portable-manifest.json",
];
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn entries() -> Vec<(String, Vec<u8>)> {
    let mut result: Vec<_> = NAMES[..7]
        .iter()
        .map(|name| ((*name).to_owned(), format!("new {name}").into_bytes()))
        .collect();
    let manifest: Vec<_> = result
        .iter()
        .map(
            |(name, content)| serde_json::json!({"name":name,"sha256":sha(content).to_uppercase()}),
        )
        .collect();
    result.push((NAMES[7].into(), serde_json::to_vec(&manifest).unwrap()));
    result
}
fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(content).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn assert_rejected(bytes: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("update.zip");
    fs::write(&path, bytes).unwrap();
    assert!(
        verify_archive(&path, &sha(bytes)).is_err(),
        "unsafe archive accepted"
    );
}
#[test]
fn valid_archive_stages_exact_payload() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("update.zip");
    let data = zip_bytes(&entries());
    fs::write(&archive, &data).unwrap();
    let stage = dir.path().join("stage");
    fs::create_dir(&stage).unwrap();
    stage_archive(&archive, &sha(&data), &stage).unwrap();
    for (name, expected) in entries() {
        assert_eq!(fs::read(stage.join(name)).unwrap(), expected);
    }
}
#[test]
fn rejects_wrong_archive_digest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("update.zip");
    fs::write(&path, zip_bytes(&entries())).unwrap();
    assert!(verify_archive(&path, &"0".repeat(64)).is_err());
}
#[test]
fn rejects_invalid_digest_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("update.zip");
    fs::write(&path, zip_bytes(&entries())).unwrap();
    for value in ["", "abc", &"g".repeat(64)] {
        assert!(verify_archive(&path, value).is_err());
    }
}
#[test]
fn rejects_truncated_zip() {
    let bytes = zip_bytes(&entries());
    assert_rejected(&bytes[..bytes.len() - 20]);
}
#[test]
fn rejects_paths_and_unlisted_files() {
    for name in [
        "../dji4g-panel.exe",
        "folder/dji4g-panel.exe",
        "folder\\dji4g-panel.exe",
        "C:\\dji4g-panel.exe",
        "dji4g-panel.exe:stream",
        "DJI4G-PANEL.EXE",
        "config.toml",
    ] {
        let mut data = entries();
        data[0].0 = name.into();
        assert_rejected(&zip_bytes(&data));
    }
}
#[test]
fn rejects_missing_and_extra_payloads() {
    let mut data = entries();
    data.remove(1);
    assert_rejected(&zip_bytes(&data));
    let mut data = entries();
    data.push(("extra.txt".into(), b"extra".to_vec()));
    assert_rejected(&zip_bytes(&data));
}
#[test]
fn rejects_manifest_hash_mismatch() {
    let mut data = entries();
    data[0].1 = b"corrupt exe".to_vec();
    assert_rejected(&zip_bytes(&data));
}
#[test]
fn rejects_manifest_missing_duplicate_and_extra_names() {
    for kind in 0..3 {
        let mut data = entries();
        let mut manifest: Vec<serde_json::Value> = serde_json::from_slice(&data[7].1).unwrap();
        match kind {
            0 => {
                manifest.pop();
            }
            1 => {
                manifest[1] = manifest[0].clone();
            }
            _ => {
                manifest[1]["name"] = "config.toml".into();
            }
        }
        data[7].1 = serde_json::to_vec(&manifest).unwrap();
        assert_rejected(&zip_bytes(&data));
    }
}
#[test]
fn rejects_symlink_zip_entry() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("dji4g-panel.exe", "elsewhere", SimpleFileOptions::default())
        .unwrap();
    for (name, data) in entries().into_iter().skip(1) {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&data).unwrap();
    }
    assert_rejected(&zip.finish().unwrap().into_inner());
}
#[test]
fn rejects_duplicate_zip_names() {
    let mut data = entries();
    data[1].0 = "dji4g-panex.exe".into();
    let mut bytes = zip_bytes(&data);
    let from = b"dji4g-panex.exe";
    let to = b"dji4g-panel.exe";
    for offset in 0..=bytes.len() - from.len() {
        if &bytes[offset..offset + from.len()] == from {
            bytes[offset..offset + from.len()].copy_from_slice(to);
        }
    }
    assert_rejected(&bytes);
}
#[test]
fn rejects_oversized_archive_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.zip");
    let file = fs::File::create(&path).unwrap();
    file.set_len(512 * 1024 * 1024 + 1).unwrap();
    assert!(verify_archive(&path, &"0".repeat(64)).is_err());
}
fn setup_transaction() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("install");
    let stage = dir.path().join("stage");
    let backup = dir.path().join("backup");
    for path in [&install, &stage, &backup] {
        fs::create_dir(path).unwrap();
    }
    for name in NAMES {
        fs::write(install.join(name), format!("old {name}")).unwrap();
        fs::write(stage.join(name), format!("new {name}")).unwrap();
    }
    fs::write(install.join("config.toml"), "user settings").unwrap();
    fs::create_dir(install.join("logs")).unwrap();
    fs::write(install.join("logs/log.txt"), "user log").unwrap();
    (dir, install, stage, backup)
}
fn assert_old(install: &Path) {
    for name in NAMES {
        assert_eq!(
            fs::read_to_string(install.join(name)).unwrap(),
            format!("old {name}")
        );
    }
    assert_eq!(
        fs::read_to_string(install.join("config.toml")).unwrap(),
        "user settings"
    );
    assert_eq!(
        fs::read_to_string(install.join("logs/log.txt")).unwrap(),
        "user log"
    );
}
#[test]
fn successful_transaction_preserves_user_files() {
    let (_dir, install, stage, backup) = setup_transaction();
    replace_and_confirm(&install, &stage, &backup, || Ok(())).unwrap();
    for name in NAMES {
        assert_eq!(
            fs::read_to_string(install.join(name)).unwrap(),
            format!("new {name}")
        );
    }
    assert_eq!(
        fs::read_to_string(install.join("config.toml")).unwrap(),
        "user settings"
    );
    assert_eq!(
        fs::read_to_string(install.join("logs/log.txt")).unwrap(),
        "user log"
    );
}
#[test]
fn startup_failure_restores_all_original_files() {
    let (_dir, install, stage, backup) = setup_transaction();
    assert!(
        replace_and_confirm(&install, &stage, &backup, || Err(UpdateError(
            "startup timeout".into()
        )))
        .is_err()
    );
    assert_old(&install);
}
#[test]
fn missing_staged_file_does_not_leave_partial_install() {
    let (_dir, install, stage, backup) = setup_transaction();
    fs::remove_file(stage.join(NAMES[3])).unwrap();
    assert!(replace_and_confirm(&install, &stage, &backup, || Ok(())).is_err());
    assert_old(&install);
}
#[cfg(windows)]
#[test]
fn locked_payload_rolls_back_prior_replacements() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_dir, install, stage, backup) = setup_transaction();
    let _locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(install.join(NAMES[3]))
        .unwrap();
    assert!(replace_and_confirm(&install, &stage, &backup, || Ok(())).is_err());
    assert_old(&install);
}
#[test]
fn rollback_removes_payload_that_was_absent_before_update() {
    let (_dir, install, stage, backup) = setup_transaction();
    fs::remove_file(install.join(NAMES[2])).unwrap();
    assert!(
        replace_and_confirm(&install, &stage, &backup, || Err(UpdateError(
            "failed".into()
        )))
        .is_err()
    );
    assert!(!install.join(NAMES[2]).exists());
    assert_eq!(
        fs::read_to_string(install.join(NAMES[0])).unwrap(),
        format!("old {}", NAMES[0])
    );
}
#[test]
fn an_unstoppable_new_process_keeps_complete_new_payload_and_old_backups() {
    let (_dir, install, stage, backup) = setup_transaction();
    let result = replace_and_confirm(&install, &stage, &backup, || {
        Err(UpdateError(
            "PROCESS_UNSTOPPED: simulated termination denial".into(),
        ))
    });
    assert!(result.unwrap_err().0.starts_with("RECOVERY_REQUIRED:"));
    for name in NAMES {
        assert_eq!(
            fs::read_to_string(install.join(name)).unwrap(),
            format!("new {name}")
        );
        assert_eq!(
            fs::read_to_string(backup.join(name)).unwrap(),
            format!("old {name}")
        );
    }
    assert_eq!(
        fs::read_to_string(install.join("config.toml")).unwrap(),
        "user settings"
    );
}
#[cfg(windows)]
#[test]
fn a_second_updater_cannot_acquire_the_same_installation_lock() {
    let dir = tempfile::tempdir().unwrap();
    let first = dji4g_updater::install::UpdateLock::acquire(dir.path()).unwrap();
    assert!(dji4g_updater::install::UpdateLock::acquire(dir.path()).is_err());
    drop(first);
    assert!(dji4g_updater::install::UpdateLock::acquire(dir.path()).is_ok());
}
