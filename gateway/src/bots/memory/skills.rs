//! Skill files for one Bot. Listing uses the workspace, not the fact store.
use super::{MemoryScopeError, MemoryStore};
use crate::storage::workspace::EntryKind;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillText {
    pub name: String,
    pub text: String,
    pub truncated: bool,
}

pub(crate) fn skill_file_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        && name.len() > ".md".len()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name != "."
        && name != ".."
}

pub(super) async fn read_skills(
    store: &MemoryStore,
    slug: &str,
    cap: usize,
) -> io::Result<Vec<SkillText>> {
    match store.require_bot_dir(slug).await {
        Ok(()) => {}
        Err(MemoryScopeError::Absent) => return Ok(Vec::new()),
        Err(MemoryScopeError::Failed(message)) => return Err(io::Error::other(message)),
    }
    let dir = format!("{slug}/skills");
    match store.ws.symlink_kind(&dir).await? {
        Some(EntryKind::Dir) => {}
        None | Some(EntryKind::File) | Some(EntryKind::Symlink) => return Ok(Vec::new()),
    }
    let mut names = Vec::new();
    for entry in store.ws.list(&dir).await? {
        if skill_file_name(&entry.name) {
            names.push(entry.name);
        }
    }
    names.sort();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let rel = format!("{dir}/{name}");
        if store.ws.symlink_kind(&rel).await? != Some(EntryKind::File) {
            continue;
        }
        let (text, truncated) = store.read_capped(&rel, cap).await?;
        out.push(SkillText {
            name,
            text,
            truncated,
        });
    }
    Ok(out)
}
