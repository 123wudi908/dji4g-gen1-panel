use crate::{UpdateError, invalid};
use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Arguments {
    pub panel_pid: u32,
    pub archive: PathBuf,
    pub sha256: String,
    pub install_dir: PathBuf,
    pub work_dir: PathBuf,
}
impl Arguments {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, UpdateError> {
        let mut args = args.into_iter();
        let mut values = HashMap::new();
        while let Some(key) = args.next() {
            let key = key
                .to_str()
                .ok_or_else(|| invalid("ARGUMENTS: non-UTF8 flag"))?
                .to_owned();
            if ![
                "--panel-pid",
                "--archive",
                "--sha256",
                "--install-dir",
                "--work-dir",
            ]
            .contains(&key.as_str())
            {
                return Err(invalid("ARGUMENTS: unknown flag"));
            }
            let value = args
                .next()
                .ok_or_else(|| invalid("ARGUMENTS: missing value"))?;
            if values.insert(key, value).is_some() {
                return Err(invalid("ARGUMENTS: duplicate flag"));
            }
        }
        if values.len() != 5 {
            return Err(invalid("ARGUMENTS: five required flags are missing"));
        }
        let panel_pid = values["--panel-pid"]
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|&n| n != 0)
            .ok_or_else(|| invalid("ARGUMENTS: invalid Panel PID"))?;
        let sha256 = values["--sha256"]
            .to_str()
            .filter(|s| crate::archive::valid_hash(s))
            .ok_or_else(|| invalid("ARGUMENTS: invalid SHA-256"))?
            .to_owned();
        Ok(Self {
            panel_pid,
            archive: PathBuf::from(&values["--archive"]),
            sha256,
            install_dir: PathBuf::from(&values["--install-dir"]),
            work_dir: PathBuf::from(&values["--work-dir"]),
        })
    }
    pub fn validate(&self) -> Result<(), UpdateError> {
        validate_work_dir(&self.work_dir)?;
        crate::install::preflight(&self.install_dir)?;
        crate::install::check_plain_path(&self.archive)?;
        let work = fs::canonicalize(&self.work_dir)?;
        if fs::canonicalize(&self.archive)?.parent() != Some(work.as_path())
            || !self.archive.is_file()
        {
            return Err(invalid(
                "SESSION: archive must be a direct child of work directory",
            ));
        }
        let install = fs::canonicalize(&self.install_dir)?;
        if install.starts_with(&work) || work.starts_with(&install) {
            return Err(invalid("SESSION: work and install directories overlap"));
        }
        for name in ["ready", "error", "startup-ok", "done"] {
            match fs::symlink_metadata(self.work_dir.join(name)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                _ => return Err(invalid("SESSION: a stale marker already exists")),
            }
        }
        Ok(())
    }
}
pub fn validate_work_dir(work: &Path) -> Result<(), UpdateError> {
    crate::install::check_plain_path(work)?;
    let canonical = fs::canonicalize(work)?;
    let temp = fs::canonicalize(std::env::temp_dir())?;
    if !canonical.is_dir()
        || canonical.parent() != Some(temp.as_path())
        || !canonical
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("dji4g-update-") && s.len() > "dji4g-update-".len())
    {
        return Err(invalid(
            "SESSION: expected a fresh dji4g-update-* direct child of temp",
        ));
    }
    Ok(())
}
pub fn write_marker(work: &Path, name: &str, content: &str) -> Result<(), UpdateError> {
    if !["ready", "error", "done"].contains(&name) {
        return Err(invalid("MARKER: unknown updater marker"));
    }
    validate_work_dir(work)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(work.join(name))?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

#[cfg(windows)]
pub fn run(args: &Arguments) -> Result<(), UpdateError> {
    use crate::{archive, install, process};
    use std::time::{Duration, Instant};
    args.validate()?;
    let install = fs::canonicalize(&args.install_dir)?;
    let work = fs::canonicalize(&args.work_dir)?;
    let _lock = install::UpdateLock::acquire(&install)?;
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    if executable.starts_with(&install) || executable.parent() != Some(work.as_path()) {
        return Err(invalid(
            "WORKER: updater must run from its temporary session",
        ));
    }
    let panel =
        process::RetainedProcess::open_expected(args.panel_pid, &install.join("dji4g-panel.exe"))?;
    let mut helpers = process::helpers(&install)?;
    let transaction = tempfile::Builder::new()
        .prefix(".dji4g-update-")
        .tempdir_in(&install)?;
    let stage = transaction.path().join("staged");
    let backup = transaction.path().join("backup");
    fs::create_dir(&stage)?;
    fs::create_dir(&backup)?;
    archive::stage_archive(&args.archive, &args.sha256, &stage)?;
    install::preflight(&install)?;
    write_marker(&work, "ready", "ready")?;
    let deadline = Instant::now() + Duration::from_secs(60);
    panel.wait(deadline.saturating_duration_since(Instant::now()))?;
    // Capture helpers started during staging before Panel's explicit safe exit.
    let helper_wait = (|| -> Result<(), UpdateError> {
        helpers.extend(process::helpers(&install)?);
        for helper in helpers {
            helper.wait(deadline.saturating_duration_since(Instant::now()))?;
        }
        Ok(())
    })();
    if let Err(error) = helper_wait {
        // Panel has already exited, but no release file has changed. Bring the old app back
        // even when a helper remains alive; never terminate that existing helper.
        let restart = process::start_panel(&install, None);
        return Err(invalid(match restart {
            Ok(_) => format!("PREINSTALL_FAILED: {error}; old Panel restarted"),
            Err(restart) => {
                format!("PREINSTALL_FAILED: {error}; OLD_PANEL_RESTART_FAILED: {restart}")
            }
        }));
    }
    let marker = work.join("startup-ok");
    let result = install::replace_and_confirm(&install, &stage, &backup, || {
        let mut child = process::start_panel(&install, Some(&marker))?;
        process::confirm_startup(&mut child, &marker, Duration::from_secs(30))
    });
    match result {
        Ok(()) => {
            transaction.close()?;
            write_marker(&work, "done", "done")?;
            Ok(())
        }
        Err(error) => {
            // Retain only this updater's transaction directory. Never enumerate user data.
            let retained = transaction.keep();
            let mut message = format!("{error}; transaction retained at {}", retained.display());
            if !error.0.starts_with("RECOVERY_REQUIRED:") {
                if let Err(restart) = process::start_panel(&install, None) {
                    message.push_str(&format!("; OLD_PANEL_RESTART_FAILED: {restart}"));
                }
            }
            Err(invalid(message))
        }
    }
}
