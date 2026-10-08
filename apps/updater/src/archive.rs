use crate::{PAYLOAD, UpdateError, invalid};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
const MAX_MANIFEST: u64 = 64 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    name: String,
    sha256: String,
}

pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
/// Stage only into a fresh empty directory owned by this update transaction.
pub fn stage_archive(path: &Path, sha: &str, destination: &Path) -> Result<(), UpdateError> {
    crate::install::check_plain_path(destination)?;
    if !destination.is_dir() || fs::read_dir(destination)?.next().is_some() {
        return Err(invalid(
            "STAGE: destination must be an empty ordinary directory",
        ));
    }
    validate(path, sha, Some(destination))
}

// zip-rs indexes files by name; inspecting the raw directory also rejects duplicate
// records which could otherwise disappear from that index. The portable format never
// needs multi-disk or ZIP64: each bound is below classic ZIP's 32-bit size limit.
fn check_directory(file: &mut File, size: u64) -> Result<(), UpdateError> {
    let tail_len = size.min(65_557) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..].starts_with(b"PK\x05\x06")
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail.len()
        })
        .ok_or_else(|| invalid("ZIP_DIRECTORY: missing end record"))?;
    let end = &tail[eocd..];
    let u16_at = |i| u16::from_le_bytes([end[i], end[i + 1]]);
    let u32_at = |i| u32::from_le_bytes(end[i..i + 4].try_into().unwrap());
    if u16_at(4) != 0 || u16_at(6) != 0 || u16_at(8) != 8 || u16_at(10) != 8 {
        return Err(invalid(
            "ZIP_DIRECTORY: expected exactly eight records on one disk",
        ));
    }
    let directory_size = u32_at(12) as u64;
    let directory_offset = u32_at(16) as u64;
    if directory_size > 1024 * 1024
        || directory_offset + directory_size != size - tail_len as u64 + eocd as u64
    {
        return Err(invalid("ZIP_DIRECTORY: invalid central directory bounds"));
    }
    file.seek(SeekFrom::Start(directory_offset))?;
    let mut directory = vec![0; directory_size as usize];
    file.read_exact(&mut directory)?;
    let mut offset = 0;
    let mut seen = HashSet::new();
    for _ in 0..8 {
        let h = directory
            .get(offset..offset + 46)
            .ok_or_else(|| invalid("ZIP_DIRECTORY: truncated entry"))?;
        if &h[..4] != b"PK\x01\x02" {
            return Err(invalid("ZIP_DIRECTORY: invalid entry signature"));
        }
        let word = |i| u16::from_le_bytes([h[i], h[i + 1]]) as usize;
        let name_length = word(28);
        let record_size = 46 + name_length + word(30) + word(32);
        if word(34) != 0 || offset + record_size > directory.len() {
            return Err(invalid("ZIP_DIRECTORY: invalid entry bounds"));
        }
        let name = std::str::from_utf8(&directory[offset + 46..offset + 46 + name_length])
            .map_err(|_| invalid("ZIP_NAME: non UTF-8 name"))?;
        if !PAYLOAD.contains(&name) || !seen.insert(name.to_owned()) {
            return Err(invalid("ZIP_NAME: unlisted or duplicate file"));
        }
        offset += record_size;
    }
    if offset != directory.len() {
        return Err(invalid("ZIP_DIRECTORY: trailing records"));
    }
    file.rewind()?;
    Ok(())
}

pub(crate) fn validate(
    path: &Path,
    expected: &str,
    destination: Option<&Path>,
) -> Result<(), UpdateError> {
    if !valid_hash(expected) {
        return Err(invalid(
            "SHA256: expected exactly 64 hexadecimal characters",
        ));
    }
    crate::install::check_plain_path(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    // Keep the archive immutable throughout both passes on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_ARCHIVE {
        return Err(invalid("ARCHIVE_SIZE: limit is 512 MiB"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected) {
        return Err(invalid("SHA256: archive digest mismatch"));
    }
    check_directory(&mut file, metadata.len())?;
    let mut zip = zip::ZipArchive::new(file)?;
    if zip.len() != PAYLOAD.len() {
        return Err(invalid("ZIP_FILES: incomplete payload"));
    }
    let mut total = 0u64;
    let mut hashes = HashMap::new();
    let mut manifest = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_owned();
        let mode_type = entry.unix_mode().unwrap_or(0) & 0o170000;
        if !PAYLOAD.contains(&name.as_str())
            || entry.is_dir()
            || entry.is_symlink()
            || entry.encrypted()
            || (mode_type != 0 && mode_type != 0o100000)
        {
            return Err(invalid(
                "ZIP_ENTRY: only ordinary allowlisted files are accepted",
            ));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| invalid("EXPANDED_SIZE: overflow"))?;
        if total > MAX_EXPANDED || (name == PAYLOAD[7] && entry.size() > MAX_MANIFEST) {
            return Err(invalid("EXPANDED_SIZE: archive or manifest too large"));
        }
        let mut output = destination
            .map(|root| {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(root.join(&name))
            })
            .transpose()?;
        let expected_size = entry.size();
        let mut actual = 0u64;
        let mut hash = Sha256::new();
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            actual += n as u64;
            if actual > expected_size {
                return Err(invalid("EXPANDED_SIZE: entry exceeds declared size"));
            }
            hash.update(&buffer[..n]);
            if name == PAYLOAD[7] {
                manifest.extend_from_slice(&buffer[..n]);
            }
            if let Some(file) = &mut output {
                file.write_all(&buffer[..n])?;
            }
        }
        if actual != expected_size {
            return Err(invalid("EXPANDED_SIZE: incomplete entry"));
        }
        if let Some(file) = output {
            file.sync_all()?;
        }
        hashes.insert(name, format!("{:x}", hash.finalize()));
    }
    // PowerShell 5's UTF8 writer may include a BOM.
    let manifest = manifest.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&manifest);
    let entries: Vec<ManifestEntry> =
        serde_json::from_slice(manifest).map_err(|e| invalid(format!("MANIFEST: {e}")))?;
    if entries.len() != 7 {
        return Err(invalid("MANIFEST: expected seven payload entries"));
    }
    let mut seen = HashSet::new();
    for entry in entries {
        if !PAYLOAD[..7].contains(&entry.name.as_str())
            || !seen.insert(entry.name.clone())
            || !valid_hash(&entry.sha256)
            || !hashes
                .get(&entry.name)
                .is_some_and(|actual| actual.eq_ignore_ascii_case(&entry.sha256))
        {
            return Err(invalid(
                "MANIFEST: unlisted, duplicate, missing or mismatched file",
            ));
        }
    }
    Ok(())
}
