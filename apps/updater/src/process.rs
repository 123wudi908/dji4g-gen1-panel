//! Windows process identity is checked against the full image path and retained handles.
use crate::{UpdateError, invalid};
use std::{
    fs,
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};

#[cfg(windows)]
mod native {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, GetLastError, HANDLE,
            INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
                QueryFullProcessImageNameW, WaitForSingleObject,
            },
        },
    };
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    pub struct RetainedProcess(Handle);
    impl RetainedProcess {
        fn open(pid: u32) -> Result<Self, UpdateError> {
            // Query and synchronize only: existing processes cannot be terminated with this handle.
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if raw.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self(Handle(raw)))
        }
        fn image(&self) -> Result<PathBuf, UpdateError> {
            let mut buffer = vec![0u16; 32768];
            let mut length = buffer.len() as u32;
            if unsafe { QueryFullProcessImageNameW(self.0.0, 0, buffer.as_mut_ptr(), &mut length) }
                == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(PathBuf::from(OsString::from_wide(
                &buffer[..length as usize],
            )))
        }
        pub fn open_expected(pid: u32, expected: &Path) -> Result<Self, UpdateError> {
            let process = Self::open(pid)?;
            if fs::canonicalize(process.image()?)? != fs::canonicalize(expected)? {
                return Err(invalid(
                    "PROCESS_IDENTITY: image does not match installation",
                ));
            }
            if unsafe { WaitForSingleObject(process.0.0, 0) } != WAIT_TIMEOUT {
                return Err(invalid("PROCESS_IDENTITY: Panel has already exited"));
            }
            Ok(process)
        }
        pub fn wait(&self, timeout: Duration) -> Result<(), UpdateError> {
            match unsafe {
                WaitForSingleObject(
                    self.0.0,
                    timeout.as_millis().min(u32::MAX as u128 - 1) as u32,
                )
            } {
                WAIT_OBJECT_0 => Ok(()),
                WAIT_TIMEOUT => Err(invalid("PROCESS_TIMEOUT: existing process did not exit")),
                _ => Err(std::io::Error::last_os_error().into()),
            }
        }
    }
    pub fn helpers(install: &Path) -> Result<Vec<RetainedProcess>, UpdateError> {
        let expected = install.join("dji4g-helper.exe");
        // Older portable distributions may omit the helper; no matching installed image can exist.
        if !expected.exists() {
            return Ok(Vec::new());
        }
        let expected = fs::canonicalize(expected)?;
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if raw == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let snapshot = Handle(raw);
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut present = unsafe { Process32FirstW(snapshot.0, &mut entry) };
        let mut result = Vec::new();
        while present != 0 {
            let n = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..n])
                .eq_ignore_ascii_case("dji4g-helper.exe")
            {
                match RetainedProcess::open(entry.th32ProcessID) {
                    Ok(process) => {
                        if unsafe { WaitForSingleObject(process.0.0, 0) } == WAIT_TIMEOUT
                            && fs::canonicalize(process.image()?)? == expected
                        {
                            result.push(process);
                        }
                    }
                    Err(e) => {
                        // A process may disappear between the snapshot and OpenProcess.
                        if unsafe { GetLastError() } != ERROR_INVALID_PARAMETER {
                            return Err(e);
                        }
                    }
                }
            }
            present = unsafe { Process32NextW(snapshot.0, &mut entry) };
        }
        if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(result)
    }
}
#[cfg(windows)]
pub use native::{RetainedProcess, helpers};

fn stop_new_child(child: &mut Child) -> Result<(), UpdateError> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    if let Err(error) = child.kill() {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        return Err(invalid(format!(
            "PROCESS_UNSTOPPED: cannot terminate newly launched Panel: {error}"
        )));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(invalid(
        "PROCESS_UNSTOPPED: newly launched Panel did not terminate",
    ))
}
/// Called only for a Child created by this updater; never looks up a PID for termination.
pub fn confirm_startup(
    child: &mut Child,
    marker: &Path,
    timeout: Duration,
) -> Result<(), UpdateError> {
    let result = (|| {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                return Err(invalid(format!(
                    "STARTUP_EXIT: Panel exited before acknowledgement: {status}"
                )));
            }
            match fs::symlink_metadata(marker) {
                Ok(metadata) => {
                    crate::install::check_plain_path(marker)?;
                    if !metadata.is_file() || metadata.len() > 4096 {
                        return Err(invalid("STARTUP_MARKER: invalid acknowledgement"));
                    }
                    return Ok(());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= deadline {
                return Err(invalid("STARTUP_TIMEOUT: no first-frame acknowledgement"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    if result.is_err() {
        stop_new_child(child).map_err(|error| {
            invalid(format!(
                "PROCESS_UNSTOPPED: cannot confirm new Panel termination: {error}"
            ))
        })?;
    }
    result
}
pub fn start_panel(install: &Path, marker: Option<&Path>) -> Result<Child, UpdateError> {
    let mut command = Command::new(install.join("dji4g-panel.exe"));
    command.current_dir(install);
    if let Some(marker) = marker {
        command.arg("--update-startup-marker").arg(marker);
    } else {
        command.arg("--update-failed");
    }
    Ok(command.spawn()?)
}
