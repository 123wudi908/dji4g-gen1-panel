use dji4g_panel::update::*;
use serde_json::json;
use std::io::{self, Read};

fn release(tag: &str) -> serde_json::Value {
    json!({"tag_name":tag, "draft":false, "prerelease":false, "assets":[
        {"name":"dji4g-panel-windows-x64-portable.zip", "size":100,
         "browser_download_url":format!("https://github.com/zhu-hailin/dji4g-gen1-panel/releases/download/{tag}/dji4g-panel-windows-x64-portable.zip")},
        {"name":"dji4g-panel-windows-x64-portable.zip.sha256", "size":107,
         "browser_download_url":format!("https://github.com/zhu-hailin/dji4g-gen1-panel/releases/download/{tag}/dji4g-panel-windows-x64-portable.zip.sha256")}
    ]})
}
fn select(value: &serde_json::Value, current: &str) -> Result<Option<Candidate>, UpdateError> {
    candidate_from_json(&serde_json::to_vec(value).unwrap(), current)
}
#[test]
fn stable_versions_use_semver_not_lexicographic_comparison() {
    for tag in ["0.1.14", "v0.1.14", "v0.1.100"] {
        assert!(select(&release(tag), "0.1.13").unwrap().is_some());
    }
    assert!(select(&release("v0.1.10"), "0.1.9").unwrap().is_some());
    for tag in ["0.1.14", "v0.1.13", "v0.1.9", "v0.1.14+build.2"] {
        assert!(select(&release(tag), "0.1.14").unwrap().is_none());
    }
}
#[test]
fn drafts_and_both_kinds_of_prereleases_are_ignored() {
    let mut value = release("v0.1.15");
    value["draft"] = json!(true);
    assert!(select(&value, "0.1.14").unwrap().is_none());
    value["draft"] = json!(false);
    value["prerelease"] = json!(true);
    assert!(select(&value, "0.1.14").unwrap().is_none());
    assert!(
        select(&release("v0.2.0-beta.1"), "0.1.14")
            .unwrap()
            .is_none()
    );
    assert!(select(&release("v0.2"), "0.1.14").is_err());
}
#[test]
fn missing_duplicate_and_oversize_assets_fail_closed() {
    let mut value = release("v0.1.15");
    value["assets"] = json!([]);
    assert!(select(&value, "0.1.14").is_err());
    let mut value = release("v0.1.15");
    let duplicate = value["assets"][0].clone();
    value["assets"].as_array_mut().unwrap().push(duplicate);
    assert!(select(&value, "0.1.14").is_err());
    for size in [0, MAX_ARCHIVE_SIZE + 1] {
        let mut value = release("v0.1.15");
        value["assets"][0]["size"] = json!(size);
        assert!(select(&value, "0.1.14").is_err());
    }
}
#[test]
fn assets_cannot_escape_repository_tag_name_or_https_origin() {
    for bad in [
        "http://github.com/zhu-hailin/dji4g-gen1-panel/releases/download/v0.1.15/dji4g-panel-windows-x64-portable.zip",
        "https://github.com/evil/dji4g-gen1-panel/releases/download/v0.1.15/dji4g-panel-windows-x64-portable.zip",
        "https://github.com/zhu-hailin/dji4g-gen1-panel/releases/download/v0.1.16/dji4g-panel-windows-x64-portable.zip",
        "https://github.com/zhu-hailin/dji4g-gen1-panel/releases/download/v0.1.15/dji4g-panel-windows-x64-local-offline.zip",
        "https://github.com/zhu-hailin/dji4g-gen1-panel/releases/download/v0.1.15/dji4g-panel-windows-x64-portable.zip?token=x",
    ] {
        let mut value = release("v0.1.15");
        value["assets"][0]["browser_download_url"] = json!(bad);
        assert!(select(&value, "0.1.14").is_err(), "accepted {bad}");
    }
}
#[test]
fn checksum_requires_exact_filename_and_one_hex_digest() {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    for prefix in ["", "\u{feff}"] {
        assert_eq!(
            parse_checksum(
                format!("{prefix}{digest}  dji4g-panel-windows-x64-portable.zip\r\n").as_bytes()
            )
            .unwrap(),
            digest
        );
    }
    assert_eq!(
        parse_checksum(
            format!(
                "{} *dji4g-panel-windows-x64-portable.zip\n",
                digest.to_uppercase()
            )
            .as_bytes()
        )
        .unwrap(),
        digest
    );
    for value in [
        digest.to_string(),
        format!("{digest} other.zip"),
        format!(
            "{digest} dji4g-panel-windows-x64-portable.zip\n{digest} dji4g-panel-windows-x64-portable.zip"
        ),
        "z".repeat(64),
        "a".repeat(4097),
    ] {
        assert!(parse_checksum(value.as_bytes()).is_err());
    }
}
#[test]
fn download_size_and_stream_errors_never_complete_successfully() {
    let mut bytes = Vec::new();
    let mut progress = Vec::new();
    write_download(&b"abcd"[..], &mut bytes, 4, |n| progress.push(n)).unwrap();
    assert_eq!(bytes, b"abcd");
    assert_eq!(progress.last(), Some(&4));
    assert!(write_download(&b"abc"[..], Vec::new(), 4, |_| {}).is_err());
    assert!(write_download(&b"abcde"[..], Vec::new(), 4, |_| {}).is_err());
    struct Interrupted;
    impl Read for Interrupted {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "interrupted",
            ))
        }
    }
    assert!(write_download(Interrupted, Vec::new(), 4, |_| {}).is_err());
}
#[test]
fn redirect_policy_accepts_only_github_release_cdn_over_https() {
    for url in [
        "https://github.com/path",
        "https://release-assets.githubusercontent.com/path?signature=opaque",
        "https://objects.githubusercontent.com/path",
    ] {
        assert!(validate_redirect(&reqwest::Url::parse(url).unwrap()));
    }
    for url in [
        "http://github.com/path",
        "https://github.com.evil.test/path",
        "https://evil.test/path",
        "https://user:pass@github.com/path",
        "https://github.com:444/path",
        "file:///tmp/x",
    ] {
        assert!(!validate_redirect(&reqwest::Url::parse(url).unwrap()));
    }
}
#[test]
fn startup_ack_argument_is_removed_from_existing_startup_options() {
    let (args, marker) = split_startup_marker(vec![
        "--autostart".into(),
        "--update-startup-marker".into(),
        "some/session/startup-ok".into(),
    ])
    .unwrap();
    assert_eq!(args, vec!["--autostart"]);
    assert_eq!(
        marker.unwrap(),
        std::path::PathBuf::from("some/session/startup-ok")
    );
    assert!(split_startup_marker(vec!["--update-startup-marker".into()]).is_err());
    assert!(
        split_startup_marker(vec![
            "--update-startup-marker".into(),
            "x".into(),
            "--update-startup-marker".into(),
            "y".into()
        ])
        .is_err()
    );
}
#[test]
fn startup_ack_cannot_write_an_arbitrary_user_file() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("settings.toml");
    assert!(write_startup_ack(&marker).is_err());
    assert!(!marker.exists());
    let session = tempfile::Builder::new()
        .prefix("dji4g-update-")
        .tempdir()
        .unwrap();
    let marker = session.path().join("startup-ok");
    assert!(write_startup_ack(&marker).is_err());
    std::fs::write(session.path().join("ready"), b"ready").unwrap();
    write_startup_ack(&marker).unwrap();
    assert!(marker.is_file());
}
