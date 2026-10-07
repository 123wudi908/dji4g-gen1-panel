#![cfg_attr(windows, windows_subsystem = "windows")]
//! The unelevated outer process owns the return path. The elevated helper only installs.
use dji4g_panel::localization::Language;
use dji4g_windows_platform::driver_setup::{self, DriverSetupOutcome as Outcome};
use std::{io::Write, process::Command};

/// One catalog string in the language this page was rendered with.
fn t(
    language: dji4g_panel::localization::Language,
    key: dji4g_panel::localization::TextKey,
) -> String {
    dji4g_panel::localization::LocalizedText::new(language, key).text
}

const SCRIPT: &str = include_str!("../../../../packaging/scripts/local-driver-install.ps1");

/// The catalog text for one stable code from the driver helper.
fn code_text(language: Language, code: &str) -> String {
    dji4g_panel::localization::stable_code_display(language, code)
}

fn panel_pid(argument: Option<&str>) -> Option<u32> {
    argument?
        .strip_prefix("--wait-for-panel=")?
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid != 0 && *pid != std::process::id())
}

fn main() {
    let language = dji4g_panel::localization::configured_language();
    let argument = std::env::args().nth(1);
    let check = matches!(argument.as_deref(), Some("--check" | "--plan"));
    if std::env::args().len() > 2
        || !(matches!(
            argument.as_deref(),
            None | Some("--check" | "--plan" | "--install")
        ) || panel_pid(argument.as_deref()).is_some())
    {
        eprintln!("Usage: dji4g-driver-setup.exe [--check | --plan | --install]");
        std::process::exit(64);
    }
    if check {
        let result = run_script(language, argument.as_deref().unwrap());
        if let Err(error) = &result {
            eprintln!("{error}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if argument.as_deref() == Some("--install") {
        let (outcome, note) = match run_script(language, "--install") {
            Ok(report) => report,
            Err(error) => (Outcome::Failed, error),
        };
        dji4g_windows_platform::show_message_box(
            &t(
                language,
                dji4g_panel::localization::TextKey::DriverResultTitle,
            ),
            &dji4g_panel::localization::format_positional(
                language,
                dji4g_panel::localization::TextKey::DriverResultBody,
                &[&code_text(language, outcome.code()), &note],
            ),
        );
        std::process::exit(outcome.exit_code() as i32);
    }
    if let Some(pid) = panel_pid(argument.as_deref()) {
        let waited = std::env::current_exe().and_then(|exe| {
            driver_setup::wait_for_panel_exit(
                pid,
                &exe.with_file_name("dji4g-panel.exe"),
                std::time::Duration::from_secs(30),
            )
        });
        if let Err(error) = waited {
            // A driver-setup error always carries a stable code as its message, so the reason can
            // be told apart: the panel was still running, was not this directory's panel, or did
            // not exit in time.
            // Do not open a duplicate panel while the existing one may still own the serial port.
            dji4g_windows_platform::show_message_box(
                &t(
                    language,
                    dji4g_panel::localization::TextKey::DriverNotStarted,
                ),
                &code_text(language, &error.to_string()),
            );
            std::process::exit(1);
        }
    }
    if argument.is_none()
        && !dji4g_windows_platform::confirm_message_box(
            None,
            &t(
                language,
                dji4g_panel::localization::TextKey::DriverInstallTitle,
            ),
            &t(
                language,
                dji4g_panel::localization::TextKey::DriverInstallBody,
            ),
        )
    {
        return_to_panel(language, Outcome::Cancelled);
        std::process::exit(1223);
    }
    let outcome = match driver_setup::elevate_current_driver_installer() {
        Ok(code) => Outcome::from_exit_code(code),
        Err(error) => {
            let result = if error.raw_os_error() == Some(1223) {
                Outcome::Cancelled
            } else {
                Outcome::Failed
            };
            dji4g_windows_platform::show_message_box(
                &t(
                    language,
                    dji4g_panel::localization::TextKey::DriverElevationFailed,
                ),
                &code_text(language, result.code()),
            );
            result
        }
    };
    return_to_panel(language, outcome);
    std::process::exit(outcome.exit_code() as i32);
}

fn return_to_panel(language: Language, outcome: Outcome) {
    if driver_setup::reopen_panel(outcome).is_err() {
        dji4g_windows_platform::show_message_box(
            &t(
                language,
                dji4g_panel::localization::TextKey::DriverOpenPanel,
            ),
            &dji4g_panel::localization::format_positional(
                language,
                dji4g_panel::localization::TextKey::DriverNoReturn,
                &[&code_text(language, outcome.code())],
            ),
        );
    }
}

fn run_script(language: Language, mode_argument: &str) -> Result<(Outcome, String), String> {
    let exe = std::env::current_exe().map_err(|_| {
        t(
            language,
            dji4g_panel::localization::TextKey::DriverNoExePath,
        )
        .to_owned()
    })?;
    let root = exe
        .parent()
        .ok_or(t(
            language,
            dji4g_panel::localization::TextKey::DriverNoExeDir,
        ))?
        .join("drivers");
    let shell = dji4g_windows_platform::driver_setup_powershell().map_err(|_| {
        t(
            language,
            dji4g_panel::localization::TextKey::DriverCheckComponentFailed,
        )
        .to_owned()
    })?;
    let mode = match mode_argument {
        "--check" => "check",
        "--plan" => "plan",
        _ => "install",
    };
    let check = mode != "install";
    let mut log = if check {
        None
    } else {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                t(
                    language,
                    dji4g_panel::localization::TextKey::DriverClockInvalid,
                )
            })?
            .as_nanos();
        let path = exe.with_file_name(format!("driver-setup-{stamp}-{}.log", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| {
                t(
                    language,
                    dji4g_panel::localization::TextKey::DriverLogCreateFailed,
                )
            })?;
        writeln!(file, "Driver setup started; mode={mode}")
            .and_then(|()| file.flush())
            .map_err(|_| {
                t(
                    language,
                    dji4g_panel::localization::TextKey::DriverLogWriteFailed,
                )
            })?;
        Some((path, file))
    };
    let mut command = Command::new(shell);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .env_remove("PSModulePath")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("DJI4G_DRIVER_ROOT", root)
        .env("DJI4G_DRIVER_MODE", mode)
        .output();
    let report = match &output {
        Ok(value) => format!(
            "Exit: {}\n{}\n{}",
            value.status,
            String::from_utf8_lossy(&value.stdout),
            String::from_utf8_lossy(&value.stderr)
        ),
        Err(error) => format!("Failed to launch driver check: {error}"),
    };
    let log_note = if let Some((path, file)) = &mut log {
        writeln!(file, "{report}")
            .and_then(|()| file.flush())
            .map_err(|_| {
                dji4g_panel::localization::format_positional(
                    language,
                    dji4g_panel::localization::TextKey::DriverLogAppendFailed,
                    &[&path.display().to_string()],
                )
            })?;
        dji4g_panel::localization::format_positional(
            language,
            dji4g_panel::localization::TextKey::DriverLogPath,
            &[&path.display().to_string()],
        )
    } else {
        String::new()
    };
    let output = output.map_err(|_| {
        dji4g_panel::localization::format_positional(
            language,
            dji4g_panel::localization::TextKey::DriverCheckNotStarted,
            &[&log_note],
        )
    })?;
    if check {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        return if output.status.success() {
            Ok((Outcome::Ready, String::new()))
        } else {
            Err(report)
        };
    }
    Ok((
        Outcome::from_script_output(
            &String::from_utf8_lossy(&output.stdout),
            output.status.success(),
        ),
        log_note,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handoff_accepts_only_nonzero_process_id() {
        assert_eq!(panel_pid(Some("--wait-for-panel=42")), Some(42));
        for value in [
            "--wait-for-panel=0",
            "--wait-for-panel=-1",
            "--wait-for-panel=4294967296",
            "--wait-for-panel=42 --install",
            "--install",
        ] {
            assert_eq!(panel_pid(Some(value)), None);
        }
        assert_eq!(
            panel_pid(Some(&format!("--wait-for-panel={}", std::process::id()))),
            None
        );
    }
}
