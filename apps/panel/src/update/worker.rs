//! All network, filesystem verification and process handoff waits run outside egui.
use super::service::Event;
use super::*;
use eframe::egui;
use std::{
    fs::{self, File},
    io::Read,
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};
use tempfile::TempDir;
const API_LIMIT: u64 = 2 * 1024 * 1024;
pub(super) struct VerifiedDownload {
    pub session: TempDir,
    pub sha256: String,
}
pub(super) struct Handoff {
    pub child: std::process::Child,
}
fn client(timeout: Duration, assets: bool) -> Result<reqwest::blocking::Client, UpdateError> {
    let redirect = if assets {
        reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !validate_redirect(attempt.url()) {
                attempt.error("untrusted update redirect")
            } else {
                attempt.follow()
            }
        })
    } else {
        reqwest::redirect::Policy::none()
    };
    reqwest::blocking::Client::builder()
        .https_only(true)
        .user_agent(concat!("dji4g-panel/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .redirect(redirect)
        .build()
        .map_err(|_| UpdateError::Network)
}
fn read_limited(response: reqwest::blocking::Response, limit: u64) -> Result<Vec<u8>, UpdateError> {
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| UpdateError::Network)?;
    if bytes.len() as u64 > limit {
        return Err(UpdateError::Download);
    }
    Ok(bytes)
}
/// An owned, cancellable one-shot check. Dropping it signals cancellation without joining egui.
/// There is no sleeping scheduler or repeated HTTP task after the service is gone.
pub(super) struct CheckTask {
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    thread: std::thread::JoinHandle<()>,
}
impl CheckTask {
    pub(super) fn spawn(
        tx: SyncSender<Event>,
        ctx: egui::Context,
        request: impl std::future::Future<Output = Result<Option<Candidate>, UpdateError>>
        + Send
        + 'static,
    ) -> std::io::Result<Self> {
        use std::future::{Future, poll_fn};
        use std::task::Poll;
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("dji4g-update-check".into())
            .spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        let result = runtime.block_on(async {
                            let mut request = std::pin::pin!(request);
                            let mut cancelled = std::pin::pin!(cancelled);
                            poll_fn(|cx| {
                                if cancelled.as_mut().poll(cx).is_ready() {
                                    return Poll::Ready(None);
                                }
                                request.as_mut().poll(cx).map(Some)
                            })
                            .await
                        });
                        // DNS work uses Tokio's blocking pool. Never wait for it on cancellation.
                        runtime.shutdown_background();
                        result
                    }
                    Err(_) => Some(Err(UpdateError::Network)),
                };
                if let Some(result) = result {
                    // At most one check result is pending; do not block a cancelled job on UI.
                    let _ = tx.try_send(Event::Checked(result));
                    ctx.request_repaint();
                }
            })?;
        Ok(Self {
            cancel: Some(cancel),
            thread,
        })
    }
    pub(super) fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }
    pub(super) fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}
impl Drop for CheckTask {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(super) fn spawn_check(tx: SyncSender<Event>, ctx: egui::Context) -> std::io::Result<CheckTask> {
    CheckTask::spawn(tx, ctx, check())
}
async fn check() -> Result<Option<Candidate>, UpdateError> {
    let mut response = reqwest::Client::builder()
        .https_only(true)
        .user_agent(concat!("dji4g-panel/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| UpdateError::Network)?
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|_| UpdateError::Network)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
        if bytes.len() as u64 + chunk.len() as u64 > API_LIMIT {
            return Err(UpdateError::Download);
        }
        bytes.extend_from_slice(&chunk);
    }
    candidate_from_json(&bytes, env!("CARGO_PKG_VERSION"))
}
pub(super) fn send(tx: &SyncSender<Event>, ctx: &egui::Context, event: Event) {
    let _ = tx.send(event);
    ctx.request_repaint();
}
pub(super) fn download(
    candidate: &Candidate,
    tx: &SyncSender<Event>,
    ctx: &egui::Context,
) -> Result<VerifiedDownload, UpdateError> {
    let response = client(Duration::from_secs(30), true)?
        .get(&candidate.checksum_url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| UpdateError::Network)?;
    let sha256 = parse_checksum(&read_limited(response, CHECKSUM_LIMIT)?)?;
    let session = tempfile::Builder::new()
        .prefix("dji4g-update-")
        .tempdir()
        .map_err(|_| UpdateError::Download)?;
    let response = client(Duration::from_secs(15 * 60), true)?
        .get(&candidate.download_url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| UpdateError::Network)?;
    if response
        .content_length()
        .is_some_and(|len| len != candidate.size)
    {
        return Err(UpdateError::Download);
    }
    let mut last = Instant::now() - Duration::from_secs(1);
    store_verified(
        response,
        session,
        sha256,
        candidate.size,
        |downloaded| {
            if last.elapsed() >= Duration::from_millis(100) || downloaded == candidate.size {
                let _ = tx.try_send(Event::Progress {
                    downloaded,
                    total: candidate.size,
                });
                ctx.request_repaint();
                last = Instant::now();
            }
        },
        || send(tx, ctx, Event::Verifying),
    )
}
/// Only the installed portable layout can update in place. A copied MSIX/development image lacks
/// the portable manifest and is refused before launching any process.
fn install_directory() -> Result<PathBuf, UpdateError> {
    let exe = std::env::current_exe()
        .map_err(|_| UpdateError::Unsupported)?
        .canonicalize()
        .map_err(|_| UpdateError::Unsupported)?;
    if exe.file_name().and_then(|p| p.to_str()) != Some("dji4g-panel.exe") {
        return Err(UpdateError::Unsupported);
    }
    let dir = exe.parent().ok_or(UpdateError::Unsupported)?;
    for name in [
        "portable-manifest.json",
        "dji4g-updater.exe",
        "dji4g-helper.exe",
    ] {
        let meta = fs::symlink_metadata(dir.join(name)).map_err(|_| UpdateError::Unsupported)?;
        if !meta.is_file() || linked(&meta) {
            return Err(UpdateError::Unsupported);
        }
    }
    Ok(dir.to_path_buf())
}
pub(super) fn install(download: VerifiedDownload) -> Result<Handoff, UpdateError> {
    let dir = install_directory()?;
    // Retain a non-writable handle while copying the trusted existing updater. Command uses
    // separate argv fields throughout; no shell interpolation ever handles install paths.
    let source = dir.join("dji4g-updater.exe");
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    let mut source = options.open(source).map_err(|_| UpdateError::Updater)?;
    let worker = download.session.path().join("worker.exe");
    let mut target = File::create_new(&worker).map_err(|_| UpdateError::Updater)?;
    std::io::copy(&mut source, &mut target).map_err(|_| UpdateError::Updater)?;
    target.sync_all().map_err(|_| UpdateError::Updater)?;
    drop(target);
    drop(source);
    let mut command = std::process::Command::new(&worker);
    command
        .arg("--panel-pid")
        .arg(std::process::id().to_string())
        .arg("--archive")
        .arg(download.session.path().join(PORTABLE_ASSET))
        .arg("--sha256")
        .arg(&download.sha256)
        .arg("--install-dir")
        .arg(&dir)
        .arg("--work-dir")
        .arg(download.session.path())
        .current_dir(&dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| UpdateError::Updater)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child
            .try_wait()
            .map_err(|_| UpdateError::Updater)?
            .is_some()
        {
            return Err(UpdateError::Updater);
        }
        if download.session.path().join("error").is_file() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UpdateError::Updater);
        }
        if download.session.path().join("ready").is_file() {
            let _session = download.session.keep();
            return Ok(Handoff { child });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UpdateError::Updater);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn store_verified(
    reader: impl Read,
    session: TempDir,
    sha256: String,
    size: u64,
    progress: impl FnMut(u64),
    verifying: impl FnOnce(),
) -> Result<VerifiedDownload, UpdateError> {
    let archive = session.path().join(PORTABLE_ASSET);
    let mut file = File::create_new(&archive).map_err(|_| UpdateError::Download)?;
    write_download(reader, &mut file, size, progress)?;
    file.sync_all().map_err(|_| UpdateError::Download)?;
    drop(file);
    verifying();
    dji4g_updater::verify_archive(&archive, &sha256).map_err(|_| UpdateError::Integrity)?;
    Ok(VerifiedDownload { session, sha256 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};
    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    fn fixture() -> Vec<u8> {
        let names = [
            "dji4g-panel.exe",
            "dji4g-helper.exe",
            "dji4g-updater.exe",
            "使用说明.txt",
            "LICENSE-MIT",
            "LICENSE-APACHE",
            "THIRD-PARTY-NOTICES.txt",
        ];
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let mut manifest = Vec::new();
        for name in names {
            let bytes = format!("fixture {name}").into_bytes();
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
            manifest.push(serde_json::json!({"name":name,"sha256":digest(&bytes)}));
        }
        zip.start_file("portable-manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn failed_or_interrupted_download_cleans_only_its_owned_session() {
        let bytes = fixture();
        for mode in 0..3 {
            let session = tempfile::Builder::new()
                .prefix("dji4g-update-")
                .tempdir()
                .unwrap();
            let path = session.path().to_path_buf();
            let body = match mode {
                1 => &b"not a ZIP"[..],
                2 => &bytes[..bytes.len() - 1],
                _ => bytes.as_slice(),
            };
            let sha = if mode == 0 {
                "0".repeat(64)
            } else if mode == 1 {
                digest(body)
            } else {
                digest(&bytes)
            };
            let size = if mode == 1 {
                body.len() as u64
            } else {
                bytes.len() as u64
            };
            let result = store_verified(Cursor::new(body), session, sha, size, |_| {}, || {});
            assert!(result.is_err());
            assert!(!path.exists(), "invalid package must be removed");
        }
    }
    #[test]
    fn verified_download_reports_bytes_and_is_installable_only_after_full_zip_validation() {
        let bytes = fixture();
        let session = tempfile::Builder::new()
            .prefix("dji4g-update-")
            .tempdir()
            .unwrap();
        let path = session.path().to_path_buf();
        let mut progress = 0;
        let mut verifying = false;
        let result = store_verified(
            Cursor::new(&bytes),
            session,
            digest(&bytes),
            bytes.len() as u64,
            |n| progress = n,
            || verifying = true,
        )
        .unwrap();
        assert_eq!(progress, bytes.len() as u64);
        assert!(verifying);
        assert_eq!(std::fs::read(path.join(PORTABLE_ASSET)).unwrap(), bytes);
        drop(result);
        assert!(!path.exists());
    }
}
