#![cfg(windows)]
//! Real updater subprocess handoff. Synthetic executables never touch devices or user installs.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
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
const FIXTURE: &str = r#"
use std::{env,fs,path::PathBuf,thread,time::Duration};
fn main(){
 let args:Vec<_>=env::args().skip(1).collect();
 if args.first().map(String::as_str)==Some("--fixture-wait") {
   let stop=PathBuf::from(&args[1]);while !stop.exists(){thread::sleep(Duration::from_millis(20));}return;
 }
 let root=PathBuf::from(env::var_os("DJI4G_FIXTURE_ROOT").unwrap());
 if args.first().map(String::as_str)==Some("--update-startup-marker") {
   if env::var("DJI4G_FIXTURE_MODE").as_deref()==Ok("fail"){std::process::exit(23);}
   fs::write(&args[1],b"ok").unwrap();fs::write(root.join("new-started"),b"yes").unwrap();
 }else{fs::write(root.join("old-restarted"),b"yes").unwrap();}
 while !root.join("stop-restarted").exists(){thread::sleep(Duration::from_millis(20));}
 fs::write(root.join("restarted-exited"),b"yes").unwrap();
}
"#;
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Process {
    child: Child,
    stop: Option<PathBuf>,
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            let _ = fs::write(stop, b"exit");
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
struct Scenario {
    root: tempfile::TempDir,
    work: tempfile::TempDir,
    install: PathBuf,
    old: Vec<(String, Vec<u8>)>,
    new: Vec<(String, Vec<u8>)>,
    archive: PathBuf,
    digest: String,
}
impl Drop for Scenario {
    fn drop(&mut self) {
        let _ = fs::write(self.root.path().join("stop-restarted"), b"exit");
        std::thread::sleep(Duration::from_millis(100));
    }
}
impl Scenario {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("portable 面板");
        fs::create_dir(&install).unwrap();
        let source = root.path().join("fixture.rs");
        fs::write(&source, FIXTURE).unwrap();
        let fixture = root.path().join("fixture.exe");
        let output = Command::new("rustc")
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(&fixture)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let fixture = fs::read(fixture).unwrap();
        let updater = fs::read(env!("CARGO_BIN_EXE_dji4g-updater")).unwrap();
        let mut old = Vec::new();
        let mut new = Vec::new();
        for name in NAMES[..7].iter() {
            let executable = match *name {
                "dji4g-panel.exe" | "dji4g-helper.exe" => Some(fixture.clone()),
                "dji4g-updater.exe" => Some(updater.clone()),
                _ => None,
            };
            let old_bytes = executable
                .clone()
                .unwrap_or_else(|| format!("old {name}").into_bytes());
            let new_bytes = executable.unwrap_or_else(|| format!("new {name}").into_bytes());
            fs::write(install.join(name), &old_bytes).unwrap();
            old.push((name.to_string(), old_bytes));
            new.push((name.to_string(), new_bytes));
        }
        let manifest: Vec<_> = new
            .iter()
            .map(|(name, bytes)| serde_json::json!({"name":name,"sha256":sha(bytes)}))
            .collect();
        old.push((NAMES[7].into(), b"old manifest".to_vec()));
        new.push((NAMES[7].into(), serde_json::to_vec(&manifest).unwrap()));
        fs::write(install.join(NAMES[7]), &old[7].1).unwrap();
        fs::write(install.join("config.toml"), b"preserve settings").unwrap();
        fs::create_dir(install.join("logs")).unwrap();
        fs::write(install.join("logs/user.log"), b"preserve logs").unwrap();
        let work = tempfile::Builder::new()
            .prefix("dji4g-update-")
            .tempdir()
            .unwrap();
        let archive = work.path().join("update.zip");
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in &new {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        fs::write(&archive, &bytes).unwrap();
        fs::copy(
            env!("CARGO_BIN_EXE_dji4g-updater"),
            work.path().join("worker.exe"),
        )
        .unwrap();
        Self {
            root,
            work,
            install,
            old,
            new,
            archive,
            digest: sha(&bytes),
        }
    }
    fn process(&self, name: &str, signal: &str) -> Process {
        let stop = self.root.path().join(signal);
        let mut command = Command::new(self.install.join(name));
        command
            .arg("--fixture-wait")
            .arg(&stop)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        Process {
            child: command.spawn().unwrap(),
            stop: Some(stop),
        }
    }
    fn updater(&self, pid: u32, mode: &str) -> Process {
        let mut command = Command::new(self.work.path().join("worker.exe"));
        command
            .arg("--panel-pid")
            .arg(pid.to_string())
            .arg("--archive")
            .arg(&self.archive)
            .arg("--sha256")
            .arg(&self.digest)
            .arg("--install-dir")
            .arg(&self.install)
            .arg("--work-dir")
            .arg(self.work.path())
            .env("DJI4G_FIXTURE_ROOT", self.root.path())
            .env("DJI4G_FIXTURE_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        Process {
            child: command.spawn().unwrap(),
            stop: None,
        }
    }
    fn assert_payload(&self, expected: &[(String, Vec<u8>)]) {
        for (name, bytes) in expected {
            assert_eq!(
                fs::read(self.install.join(name)).unwrap(),
                *bytes,
                "changed {name}"
            );
        }
        assert_eq!(
            fs::read(self.install.join("config.toml")).unwrap(),
            b"preserve settings"
        );
        assert_eq!(
            fs::read(self.install.join("logs/user.log")).unwrap(),
            b"preserve logs"
        );
    }
    fn stop_panel(&self, process: &mut Process) {
        fs::write(process.stop.as_ref().unwrap(), b"exit").unwrap();
        assert!(wait_exit(&mut process.child, Duration::from_secs(5)).success());
    }
}
fn wait_file(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.is_file() {
        assert!(Instant::now() < deadline, "missing {}", path.display());
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn wait_exit(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "process {} did not exit",
            child.id()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
#[test]
fn actual_updater_waits_for_panel_and_helper_then_restarts_verified_payload() {
    let s = Scenario::new();
    let mut panel = s.process(NAMES[0], "exit-old");
    let mut helper = s.process(NAMES[1], "exit-helper");
    let mut updater = s.updater(panel.child.id(), "ack");
    wait_file(&s.work.path().join("ready"), Duration::from_secs(15));
    s.assert_payload(&s.old);
    s.stop_panel(&mut panel);
    std::thread::sleep(Duration::from_millis(200));
    s.assert_payload(&s.old);
    s.stop_panel(&mut helper);
    assert!(wait_exit(&mut updater.child, Duration::from_secs(15)).success());
    wait_file(&s.root.path().join("new-started"), Duration::from_secs(5));
    assert!(s.work.path().join("done").is_file());
    s.assert_payload(&s.new);
}
#[test]
fn actual_updater_rolls_back_failed_new_startup_and_restarts_old_panel() {
    let s = Scenario::new();
    let mut panel = s.process(NAMES[0], "exit-old");
    let mut updater = s.updater(panel.child.id(), "fail");
    wait_file(&s.work.path().join("ready"), Duration::from_secs(15));
    s.stop_panel(&mut panel);
    assert!(!wait_exit(&mut updater.child, Duration::from_secs(15)).success());
    wait_file(&s.root.path().join("old-restarted"), Duration::from_secs(5));
    s.assert_payload(&s.old);
    assert!(s.work.path().join("error").is_file());
}
#[test]
fn helper_exit_timeout_preserves_payload_and_restarts_old_panel() {
    let s = Scenario::new();
    let mut panel = s.process(NAMES[0], "exit-old");
    let _helper = s.process(NAMES[1], "exit-helper");
    let mut updater = s.updater(panel.child.id(), "ack");
    wait_file(&s.work.path().join("ready"), Duration::from_secs(15));
    s.stop_panel(&mut panel);
    assert!(!wait_exit(&mut updater.child, Duration::from_secs(70)).success());
    s.assert_payload(&s.old);
    wait_file(&s.root.path().join("old-restarted"), Duration::from_secs(5));
    assert!(s.work.path().join("error").is_file());
}
