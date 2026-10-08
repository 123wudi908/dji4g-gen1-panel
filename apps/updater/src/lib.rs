//! Standalone portable updater. This crate deliberately has no Panel, helper or device dependencies.
use std::path::Path;
pub mod archive;
pub mod install;
pub mod process;
pub mod session;

pub const PAYLOAD: [&str; 8] = [
    "dji4g-panel.exe",
    "dji4g-helper.exe",
    "dji4g-updater.exe",
    "使用说明.txt",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "THIRD-PARTY-NOTICES.txt",
    "portable-manifest.json",
];
#[derive(Debug)]
pub struct UpdateError(pub String);
impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for UpdateError {}
impl From<std::io::Error> for UpdateError {
    fn from(value: std::io::Error) -> Self {
        Self(format!("IO: {value}"))
    }
}
impl From<zip::result::ZipError> for UpdateError {
    fn from(value: zip::result::ZipError) -> Self {
        Self(format!("ZIP: {value}"))
    }
}
pub(crate) fn invalid(message: impl Into<String>) -> UpdateError {
    UpdateError(message.into())
}
/// Validate the SHA-256, complete flat ZIP allowlist, size limits and manifest hashes.
/// No extraction or install-directory modification is performed.
pub fn verify_archive(path: &Path, expected_sha256: &str) -> Result<(), UpdateError> {
    archive::validate(path, expected_sha256, None)
}
