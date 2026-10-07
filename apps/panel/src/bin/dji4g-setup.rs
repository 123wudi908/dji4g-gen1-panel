#![cfg_attr(windows, windows_subsystem = "windows")]
//! A single-file per-user installer containing the entire reviewed offline payload.
use dji4g_panel::localization::Language;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// One catalog string in the language this page was rendered with.
fn t(
    language: dji4g_panel::localization::Language,
    key: dji4g_panel::localization::TextKey,
) -> String {
    dji4g_panel::localization::LocalizedText::new(language, key).text
}
include!(concat!(env!("OUT_DIR"), "/offline_bundle.rs"));
const INSTALL: &str = include_str!("../../../../packaging/scripts/install-offline-app.ps1");

fn main() {
    let language = dji4g_panel::localization::configured_language();
    if let Err(error) = run(language) {
        dji4g_windows_platform::show_message_box(
            &t(
                language,
                dji4g_panel::localization::TextKey::SetupFailedTitle,
            ),
            &error,
        );
        std::process::exit(1);
    }
}

fn run(language: Language) -> Result<(), String> {
    if PAYLOAD.is_empty() {
        return Err(t(
            language,
            dji4g_panel::localization::TextKey::SetupNoPayload,
        ));
    }
    if !dji4g_windows_platform::confirm_message_box(
        None,
        &t(
            language,
            dji4g_panel::localization::TextKey::SetupConfirmTitle,
        ),
        &t(
            language,
            dji4g_panel::localization::TextKey::SetupConfirmBody,
        ),
    ) {
        return Ok(());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staging = std::env::temp_dir().join(format!("dji4g-setup-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let zip = staging.join("payload.zip");
    fs::write(&zip, PAYLOAD).map_err(|e| e.to_string())?;
    let shell = dji4g_windows_platform::driver_setup_powershell().map_err(|e| e.to_string())?;
    let mut command = Command::new(shell);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command
        .env_remove("PSModulePath")
        .args(["-NoProfile", "-NonInteractive", "-Command", INSTALL])
        .env("DJI4G_SETUP_PAYLOAD", &zip)
        .env("DJI4G_SETUP_HASH", HASH)
        .output()
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&zip);
    let _ = fs::remove_dir(&staging);
    if !result.status.success() {
        return Err(format!(
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let installed = String::from_utf8(result.stdout).map_err(|e| e.to_string())?;
    let directory = std::path::PathBuf::from(installed.trim());
    if !directory.join("dji4g-panel.exe").is_file() {
        return Err(t(
            language,
            dji4g_panel::localization::TextKey::SetupVerifyFailed,
        ));
    }
    if dji4g_windows_platform::confirm_message_box(
        None,
        &t(language, dji4g_panel::localization::TextKey::SetupDoneTitle),
        &t(language, dji4g_panel::localization::TextKey::SetupDoneBody),
    ) {
        let status = Command::new(directory.join("dji4g-driver-setup.exe"))
            .status()
            .map_err(|e| e.to_string())?;
        driver_result(status.code(), language)?;
    }
    Command::new(directory.join("dji4g-panel.exe"))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn driver_result(code: Option<i32>, language: Language) -> Result<(), String> {
    match code {
        Some(0) => Ok(()),
        Some(1223) => Err(t(
            language,
            dji4g_panel::localization::TextKey::SetupDriverCancelled,
        )),
        _ => Err(dji4g_panel::localization::format_positional(
            language,
            dji4g_panel::localization::TextKey::SetupDriverIncomplete,
            &[&format!("{code:?}")],
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::driver_result;

    #[test]
    fn failed_or_cancelled_driver_setup_stops_app_launch() {
        for code in [Some(1), Some(1223), Some(64), None] {
            assert!(
                driver_result(code, dji4g_panel::localization::Language::ZhCn).is_err(),
                "must stop for {code:?}"
            );
        }
    }

    #[test]
    fn successful_driver_setup_allows_app_launch() {
        assert!(driver_result(Some(0), dji4g_panel::localization::Language::ZhCn).is_ok());
    }
}
