//! Copy a folder tree and compare hashes. The source is never touched.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn dir_has_entries(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

pub(crate) fn copy_tree(source: &Path, dest: &Path) -> std::io::Result<u64> {
    std::fs::create_dir_all(dest)?;
    if !source.exists() {
        return Ok(0);
    }
    copy_children(source, dest)
}

pub(crate) fn copy_children(source: &Path, dest: &Path) -> std::io::Result<u64> {
    let mut files = 0_u64;
    if !source.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let from = entry.path();
        let to = dest.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            let target = std::fs::read_link(&from)?;
            symlink_at(&target, &to)?;
        } else if file_type.is_dir() {
            std::fs::create_dir_all(&to)?;
            files += copy_children(&from, &to)?;
        } else if file_type.is_file() {
            std::fs::copy(&from, &to)?;
            files += 1;
        }
    }
    Ok(files)
}

pub(crate) fn symlink_at(target: &Path, dest: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, dest)
    }
    #[cfg(not(unix))]
    {
        let _ = (target, dest);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks are not copied on this system",
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntryHash {
    File(String),
    Link(PathBuf),
}

pub(crate) fn tree_hashes(root: &Path) -> BTreeMap<String, EntryHash> {
    let mut out = BTreeMap::new();
    if root.is_dir() {
        hash_dir(root, "", &mut out);
    }
    out
}

pub(crate) fn hash_dir(dir: &Path, prefix: &str, out: &mut BTreeMap<String, EntryHash>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                out.insert(rel, EntryHash::Link(target));
            }
        } else if file_type.is_dir() {
            hash_dir(&path, &rel, out);
        } else if file_type.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                out.insert(rel, EntryHash::File(hex::encode(hasher.finalize())));
            }
        }
    }
}
