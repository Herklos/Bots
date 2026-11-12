//! Copies of bytes this device is about to overwrite or remove.
//! Each path keeps the last 20 revisions, and anything from the last 14 days.

use super::sha256_hex;
use crate::storage::write_atomic;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub(super) const KEEP_REVISIONS: usize = 20;
pub(super) const KEEP_FOR: Duration = Duration::from_secs(14 * 24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Revision {
    pub path: PathBuf,
    pub modified: SystemTime,
}

pub(super) async fn record(root: &Path, rel: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let dir = directory(root, rel);
    let rev = next_rev(&dir)?;
    let path = dir.join(format!("{rev:08}.bin"));
    write_atomic(&path, bytes, Some(0o600)).await?;
    prune_dir(&dir, SystemTime::now())?;
    Ok(path)
}

pub(super) fn revisions(root: &Path, rel: &str) -> io::Result<Vec<Revision>> {
    let dir = directory(root, rel);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    read_revisions(&dir)
}

pub(super) fn prune(root: &Path, rel: &str, now: SystemTime) -> io::Result<()> {
    let dir = directory(root, rel);
    if !dir.exists() {
        return Ok(());
    }
    prune_dir(&dir, now)
}

fn directory(root: &Path, rel: &str) -> PathBuf {
    root.join(sha256_hex(rel.as_bytes()))
}

fn next_rev(dir: &Path) -> io::Result<u32> {
    if !dir.exists() {
        return Ok(1);
    }
    let mut max = 0_u32;
    for revision in read_revisions(dir)? {
        let Some(name) = revision.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".bin") else {
            continue;
        };
        if let Ok(rev) = stem.parse::<u32>() {
            max = max.max(rev);
        }
    }
    Ok(max.saturating_add(1))
}

fn prune_dir(dir: &Path, now: SystemTime) -> io::Result<()> {
    let mut files = read_revisions(dir)?;
    files.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.path.cmp(&left.path))
    });
    let cutoff = now.checked_sub(KEEP_FOR).unwrap_or(SystemTime::UNIX_EPOCH);
    for (index, file) in files.iter().enumerate() {
        if index < KEEP_REVISIONS || file.modified >= cutoff {
            continue;
        }
        std::fs::remove_file(&file.path)?;
    }
    Ok(())
}

fn read_revisions(dir: &Path) -> io::Result<Vec<Revision>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_revision(name) {
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path())?;
        if !meta.is_file() {
            continue;
        }
        // A missing clock still counts as current, so a prune does not drop the bytes.
        let modified = meta.modified().unwrap_or_else(|_| SystemTime::now());
        out.push(Revision {
            path: entry.path(),
            modified,
        });
    }
    Ok(out)
}

fn is_revision(name: &str) -> bool {
    name.strip_suffix(".bin")
        .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_digit()))
}
