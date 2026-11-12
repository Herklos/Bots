//! Walk the allow-listed files. Size and mtime reuse the stored sha.
//! A file touched within the last second is left for the next pass.
//! Callers must not treat an unsettled path as deleted.

use super::policy::{can_descend, classify, Class};
use super::sha256_hex;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) const SETTLE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Shortcut {
    pub size: u64,
    pub mtime_ns: i64,
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Present {
    pub sha: String,
    pub size: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Scan {
    pub files: BTreeMap<String, Present>,
    pub unsettled: BTreeSet<String>,
    pub hashed: BTreeSet<String>,
    pub reused: BTreeSet<String>,
}

pub(super) fn scan(
    root: &Path,
    cache: &BTreeMap<String, Shortcut>,
    now: SystemTime,
    settle: Duration,
) -> io::Result<Scan> {
    let mut out = Scan {
        files: BTreeMap::new(),
        unsettled: BTreeSet::new(),
        hashed: BTreeSet::new(),
        reused: BTreeSet::new(),
    };
    walk(root, "", cache, now, settle, &mut out)?;
    Ok(out)
}

fn walk(
    dir: &Path,
    prefix: &str,
    cache: &BTreeMap<String, Shortcut>,
    now: SystemTime,
    settle: Duration,
    out: &mut Scan,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            continue;
        }
        let path = dir.join(&name);
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let meta = fs::symlink_metadata(&path)?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if can_descend(&rel) {
                walk(&path, &rel, cache, now, settle, out)?;
            }
            continue;
        }
        if kind.is_file() {
            consider_file(&path, &rel, &meta, cache, now, settle, out)?;
        }
    }
    Ok(())
}

fn consider_file(
    path: &Path,
    rel: &str,
    meta: &fs::Metadata,
    cache: &BTreeMap<String, Shortcut>,
    now: SystemTime,
    settle: Duration,
    out: &mut Scan,
) -> io::Result<()> {
    if !matches!(classify(rel), Class::Allow { .. }) {
        return Ok(());
    }
    let Some(mtime_ns) = mtime_ns(meta) else {
        return hash_file(path, rel, meta.len(), None, out);
    };
    let modified = meta.modified()?;
    if !settled(modified, now, settle) {
        out.unsettled.insert(rel.to_string());
        return Ok(());
    }
    let size = meta.len();
    if let Some(cached) = cache.get(rel) {
        if cached.size == size && cached.mtime_ns == mtime_ns {
            out.files.insert(
                rel.to_string(),
                Present {
                    sha: cached.sha.clone(),
                    size,
                    mtime_ns,
                },
            );
            out.reused.insert(rel.to_string());
            return Ok(());
        }
    }
    hash_file(path, rel, size, Some(mtime_ns), out)
}

fn hash_file(
    path: &Path,
    rel: &str,
    size: u64,
    mtime_ns: Option<i64>,
    out: &mut Scan,
) -> io::Result<()> {
    let bytes = fs::read(path)?;
    let sha = sha256_hex(&bytes);
    let mtime_ns = mtime_ns.unwrap_or(0);
    out.hashed.insert(rel.to_string());
    out.files.insert(
        rel.to_string(),
        Present {
            sha,
            size,
            mtime_ns,
        },
    );
    Ok(())
}

fn settled(modified: SystemTime, now: SystemTime, settle: Duration) -> bool {
    match now.duration_since(modified) {
        Ok(age) => age >= settle,
        Err(_) => false,
    }
}

fn mtime_ns(meta: &fs::Metadata) -> Option<i64> {
    let modified = meta.modified().ok()?;
    let nanos = modified.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    i64::try_from(nanos).ok()
}
