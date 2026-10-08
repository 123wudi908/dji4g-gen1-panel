#![cfg(windows)]
use dji4g_updater::{
    process::{RetainedProcess, confirm_startup},
    session::Arguments,
};
use std::{
    ffi::OsString,
    fs,
    process::{Command, Stdio},
    time::Duration,
};
#[test]
fn process_identity_rejects_wrong_image_and_invalid_pid() {
    assert!(
        RetainedProcess::open_expected(
            std::process::id(),
            &std::env::current_exe().unwrap().with_file_name("other.exe")
        )
        .is_err()
    );
    assert!(RetainedProcess::open_expected(0, &std::env::current_exe().unwrap()).is_err());
}
#[test]
fn retained_process_wait_is_bounded_and_does_not_kill() {
    let process =
        RetainedProcess::open_expected(std::process::id(), &std::env::current_exe().unwrap())
            .unwrap();
    assert!(process.wait(Duration::from_millis(10)).is_err());
}
fn child(mode: &str, marker: &std::path::Path) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "fixture_child"])
        .env("DJI4G_TEST_CHILD_MODE", mode)
        .env("DJI4G_TEST_MARKER", marker)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}
#[test]
#[ignore = "subprocess fixture launched by the startup tests"]
fn fixture_child() {
    match std::env::var("DJI4G_TEST_CHILD_MODE").as_deref() {
        Ok("ack") => {
            fs::write(std::env::var_os("DJI4G_TEST_MARKER").unwrap(), b"ok").unwrap();
            std::thread::sleep(Duration::from_secs(10));
        }
        Ok("hang") => std::thread::sleep(Duration::from_secs(10)),
        _ => (),
    }
}
#[test]
fn startup_timeout_terminates_only_the_new_child() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("startup-ok");
    let mut process = child("hang", &marker);
    let result = confirm_startup(&mut process, &marker, Duration::from_millis(50));
    let stopped = process.try_wait().unwrap().is_some();
    if !stopped {
        process.kill().unwrap();
        process.wait().unwrap();
    }
    assert!(result.is_err());
    assert!(stopped, "new child was not stopped before rollback");
}
#[test]
fn startup_exit_without_marker_is_failure() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("startup-ok");
    let mut process = child("exit", &marker);
    let result = confirm_startup(&mut process, &marker, Duration::from_secs(2));
    process.wait().unwrap();
    assert!(result.is_err());
}
#[test]
fn startup_acknowledgement_keeps_new_child_running() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("startup-ok");
    let mut process = child("ack", &marker);
    let result = confirm_startup(&mut process, &marker, Duration::from_secs(5));
    let marker_exists = marker.exists();
    let running = process.try_wait().unwrap().is_none();
    process.kill().unwrap();
    process.wait().unwrap();
    assert!(result.is_ok());
    assert!(marker_exists);
    assert!(running);
}
#[test]
fn cli_rejects_missing_duplicate_unknown_and_bad_pid_arguments() {
    for args in [
        vec![],
        vec!["--panel-pid", "0"],
        vec!["--unknown", "x"],
        vec!["--panel-pid", "123", "--panel-pid", "123"],
    ] {
        assert!(Arguments::parse(args.into_iter().map(OsString::from)).is_err());
    }
}
#[test]
fn cli_preserves_unicode_and_space_paths() {
    let args = [
        "--panel-pid",
        "123",
        "--archive",
        r"C:\临时 文件\update.zip",
        "--sha256",
        &"A".repeat(64),
        "--install-dir",
        r"D:\面板 portable",
        "--work-dir",
        r"C:\临时 文件",
    ];
    let result = Arguments::parse(args.into_iter().map(OsString::from)).unwrap();
    assert_eq!(result.panel_pid, 123);
    assert_eq!(result.install_dir.to_str().unwrap(), r"D:\面板 portable");
}
#[test]
fn session_refuses_arbitrary_work_directory_and_preexisting_markers() {
    let install = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("update.zip");
    fs::write(&archive, b"archive").unwrap();
    let args = Arguments {
        panel_pid: std::process::id(),
        archive,
        sha256: "A".repeat(64),
        install_dir: install.path().to_owned(),
        work_dir: dir.path().to_owned(),
    };
    assert!(args.validate().is_err());
    let dir = tempfile::Builder::new()
        .prefix("dji4g-update-")
        .tempdir()
        .unwrap();
    let archive = dir.path().join("update.zip");
    fs::write(&archive, b"archive").unwrap();
    let args = Arguments {
        work_dir: dir.path().to_owned(),
        archive,
        ..args
    };
    fs::write(dir.path().join("ready"), b"stale").unwrap();
    assert!(args.validate().is_err());
}
