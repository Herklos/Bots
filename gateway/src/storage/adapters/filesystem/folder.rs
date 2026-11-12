//! Folder rules for the Bots root: `~` expansion, canonical paths, and the write probe.
use std::path::{Path, PathBuf};

pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn expand_tilde(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(raw)
}

pub(crate) fn validate_folder(data_root: &Path, raw: &str) -> Result<PathBuf, String> {
    let expanded = expand_tilde(raw);
    if !expanded.is_absolute() {
        return Err("The folder must be an absolute path.".into());
    }
    let path = canonical_candidate(&expanded)?;
    if same_path(&path, data_root) {
        return Err("The folder cannot be the hbots home.".into());
    }
    if path.components().any(|component| {
        let name = component.as_os_str();
        name == ".botapp" || name == ".bot"
    }) {
        return Err("The folder cannot be inside a Bot folder or .botapp.".into());
    }
    if inside_bot_folder(&path) {
        return Err("The folder cannot be inside a Bot folder or .botapp.".into());
    }
    if path.is_file() {
        return Err("The folder is a file.".into());
    }
    std::fs::create_dir_all(&path).map_err(|_error| "The folder cannot be created.".to_string())?;
    let probe = path.join(".hbots-write-probe");
    std::fs::write(&probe, b"ok").map_err(|_error| "The folder is not writable.".to_string())?;
    let _ = std::fs::remove_file(&probe);
    Ok(path)
}

pub(crate) fn canonical_candidate(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return std::fs::canonicalize(path)
            .map_err(|_error| "The folder cannot be created.".to_string());
    }
    let mut suffix = Vec::new();
    let mut cursor = path;
    loop {
        if cursor.exists() {
            let mut out = std::fs::canonicalize(cursor)
                .map_err(|_error| "The folder cannot be created.".to_string())?;
            for part in suffix.iter().rev() {
                out.push(part);
            }
            return Ok(out);
        }
        match cursor.file_name() {
            Some(name) => suffix.push(name.to_os_string()),
            None => return Err("The folder must be an absolute path.".into()),
        }
        match cursor.parent() {
            Some(parent) if parent != cursor => cursor = parent,
            _ => return Err("The folder cannot be created.".into()),
        }
    }
}

pub(crate) fn same_path(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left == right
}

pub(crate) fn inside_bot_folder(path: &Path) -> bool {
    let mut cursor = Some(path);
    while let Some(dir) = cursor {
        if dir.join(".bot").join("bot.json").is_file() {
            return true;
        }
        cursor = dir.parent().filter(|parent| *parent != dir);
    }
    false
}

pub(crate) fn file_nonempty(path: &Path) -> bool {
    std::fs::metadata(path)
        .ok()
        .is_some_and(|meta| meta.is_file() && meta.len() > 0)
}
