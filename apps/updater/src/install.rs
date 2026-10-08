use crate::{PAYLOAD, UpdateError, invalid};
use std::{
    fs,
    path::{Component, Path},
};

/// Reject symlinks and Windows junctions/reparse points along every existing component.
/// This updater is for an ordinary local portable directory, not redirected installs.
pub fn check_plain_path(path: &Path) -> Result<(), UpdateError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid("PATH: expected an absolute path without traversal"));
    }
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("PATH: linked targets are not supported"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("PATH: reparse targets are not supported"));
            }
        }
    }
    Ok(())
}
fn check_file(path: &Path, allow_absent: bool) -> Result<(), UpdateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            check_plain_path(path)?;
            if !metadata.is_file() {
                return Err(invalid("PATH: payload target is not an ordinary file"));
            }
            Ok(())
        }
        Err(e) if allow_absent && e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn preflight(install: &Path) -> Result<(), UpdateError> {
    check_plain_path(install)?;
    if !install.is_dir() {
        return Err(invalid("INSTALL: expected a directory"));
    }
    for name in PAYLOAD {
        check_file(&install.join(name), true)?;
    }
    Ok(())
}

/// Move only allowlisted payload files. All backups stay on the same filesystem.
/// On any error the caller must retain the backup directory for manual recovery.
/// `confirm` must stop any process it launched before returning an error.
pub fn replace_and_confirm<F: FnOnce() -> Result<(), UpdateError>>(
    install: &Path,
    stage: &Path,
    backup: &Path,
    confirm: F,
) -> Result<(), UpdateError> {
    preflight(install)?;
    check_plain_path(stage)?;
    check_plain_path(backup)?;
    if fs::read_dir(backup)?.next().is_some() {
        return Err(invalid("BACKUP: directory must be empty"));
    }
    for name in PAYLOAD {
        check_file(&stage.join(name), false)?;
    }
    let mut moved_old = Vec::new();
    let mut installed = Vec::new();
    let operation = (|| {
        for name in PAYLOAD {
            let target = install.join(name);
            // Recheck immediately before moving; never follow a linked target.
            check_file(&target, true)?;
            if fs::symlink_metadata(&target).is_ok() {
                fs::rename(&target, backup.join(name))?;
                moved_old.push(name);
            }
            fs::rename(stage.join(name), &target)?;
            installed.push(name);
        }
        confirm()
    })();
    if let Err(cause) = operation {
        // A process we could not stop still maps the complete new payload. Removing any of
        // it would create a mixed installation. Preserve every old backup for recovery.
        if cause.0.starts_with("PROCESS_UNSTOPPED:") {
            return Err(invalid(format!(
                "RECOVERY_REQUIRED: {cause}; complete new payload retained; backups: {}",
                backup.display()
            )));
        }
        let mut failures = Vec::new();
        for name in installed.iter().rev() {
            let target = install.join(name);
            if let Err(e) = check_file(&target, false)
                .and_then(|_| fs::remove_file(&target).map_err(Into::into))
            {
                failures.push(format!("remove {name}: {e}"));
            }
        }
        for name in moved_old.iter().rev() {
            let target = install.join(name);
            // Never overwrite a file we could not remove or one created concurrently.
            if fs::symlink_metadata(&target).is_ok() {
                failures.push(format!("restore {name}: target still exists"));
                continue;
            }
            if let Err(e) = fs::rename(backup.join(name), &target) {
                failures.push(format!("restore {name}: {e}"));
            }
        }
        if failures.is_empty() {
            return Err(invalid(format!("ROLLED_BACK: {cause}")));
        }
        return Err(invalid(format!(
            "RECOVERY_REQUIRED: {cause}; backups: {}; {}",
            backup.display(),
            failures.join("; ")
        )));
    }
    Ok(())
}

/// Held across the update transaction; Windows grants only one writer in this directory.
pub struct UpdateLock {
    _file: fs::File,
}
impl UpdateLock {
    pub fn acquire(install: &Path) -> Result<Self, UpdateError> {
        check_plain_path(install)?;
        let path = install.join(".dji4g-update.lock");
        check_file(&path, true)?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let file = options.open(&path)?;
        // Persistent zero-byte lock is reusable after a crash. Do not unlink a file that
        // another updater could open concurrently; it never enters the portable payload.
        Ok(Self { _file: file })
    }
}
