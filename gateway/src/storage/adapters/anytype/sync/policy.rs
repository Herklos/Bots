//! Which relative paths may sync, and how a text file merges.
//! A pulled path is untrusted: `..` and symlinks are refused.

use crate::storage::workspace::safe_rel;
use std::io::ErrorKind;
use std::path::Path;

/// `Text3Way` merges by line. `Whole` conflicts when both sides changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MergePolicy {
    Text3Way,
    Whole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    Allow { merge: MergePolicy },
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PullReject {
    Escapes,
    Denied,
    Symlink,
    /// An ancestor is not a directory, or its metadata could not be read.
    Unavailable,
}

const SECRETS: &[&str] = &[
    "vault.enc",
    ".vault-key",
    "access.json",
    "session-secret",
    ".bot-mcp-token",
    "t3-token",
    ".env",
    "storage.json",
    "anytype-key",
];

const DENY_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", ".mcp.json"];

const DENY_DIRS: &[&str] = &[".codex", ".claude", ".git"];

pub(super) fn classify(rel: &str) -> Class {
    if safe_rel(rel).is_err() {
        return Class::Deny;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    if denied(&parts) {
        return Class::Deny;
    }
    let Some(name) = parts.last().copied() else {
        return Class::Deny;
    };
    if allowed(&parts) {
        Class::Allow {
            merge: merge_for(name),
        }
    } else {
        Class::Deny
    }
}

/// A directory the scanner may enter. `.botapp` is entered so the one shared file can be seen.
pub(super) fn can_descend(rel: &str) -> bool {
    if safe_rel(rel).is_err() {
        return false;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    if parts == [".botapp"] {
        return true;
    }
    if denied(&parts) {
        return false;
    }
    matches!(
        parts.as_slice(),
        [_] | [_, "memory", ..]
            | [_, "skills", ..]
            | [_, "library", ..]
            | [_, ".bot"]
            | [_, ".bot", "attachments", ..]
    )
}

/// `rel` is a path from a remote object. Refuse anything that is not an allowed
/// file under `root`, including a symlink at any step. Metadata is never followed.
pub(super) fn accept_pulled(root: &Path, rel: &str) -> Result<Class, PullReject> {
    if safe_rel(rel).is_err() {
        return Err(PullReject::Escapes);
    }
    let class = classify(rel);
    if matches!(class, Class::Deny) {
        return Err(PullReject::Denied);
    }
    if let Some(reject) = symlink_on_path(root, rel) {
        return Err(reject);
    }
    Ok(class)
}

fn denied(parts: &[&str]) -> bool {
    if parts == [".botapp", "shared-memory.md"] {
        return false;
    }
    if parts.iter().any(|part| {
        SECRETS.contains(part)
            || DENY_FILES.contains(part)
            || DENY_DIRS.contains(part)
            || part.contains(".conflict-")
            || extension_is(part, "tmp")
            || extension_is(part, "lock")
    }) {
        return true;
    }
    if is_bot_runtime(parts) || parts == [".botapp", "groups.local.json"] {
        return true;
    }
    parts.contains(&".botapp")
}

fn allowed(parts: &[&str]) -> bool {
    if parts == [".botapp", "shared-memory.md"] {
        return true;
    }
    let [_, area, rest @ ..] = parts else {
        return false;
    };
    matches!(
        (*area, rest),
        ("memory", [_, ..])
            | ("skills", [_, ..])
            | ("library", [_, ..])
            | (".bot", ["library.json"])
            | (".bot", ["attachments", _, ..])
    )
}

fn is_bot_runtime(parts: &[&str]) -> bool {
    parts.windows(2).any(|pair| {
        pair[0] == ".bot" && matches!(pair[1], "bot.json" | "local.json" | "connectors.json")
    })
}

fn merge_for(name: &str) -> MergePolicy {
    if extension_is(name, "md") || extension_is(name, "txt") || extension_is(name, "html") {
        MergePolicy::Text3Way
    } else {
        MergePolicy::Whole
    }
}

fn extension_is(name: &str, ext: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, found)| found.eq_ignore_ascii_case(ext))
}

fn symlink_on_path(root: &Path, rel: &str) -> Option<PullReject> {
    let parts: Vec<&str> = rel.split('/').collect();
    let mut cursor = root.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        cursor.push(part);
        let last = index + 1 == parts.len();
        match std::fs::symlink_metadata(&cursor) {
            Ok(meta) if meta.file_type().is_symlink() => return Some(PullReject::Symlink),
            Ok(meta) if meta.is_dir() => {}
            Ok(_) if last => {}
            Ok(_) => return Some(PullReject::Unavailable),
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(_) => return Some(PullReject::Unavailable),
        }
    }
    None
}
