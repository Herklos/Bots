//! Bot memory files, through the workspace. Callers pass relative keys.
//! Canonical paths stay inside `LocalDisk`.

mod skills;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use skills::skill_file_name;

use crate::bots::valid_slug;
use crate::storage::workspace::{EntryKind, Workspace};
use std::io;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum MemoryScopeError {
    /// Missing, not a directory, or a symlink. Tools treat this as an absent folder.
    Absent,
    Failed(String),
}

impl std::fmt::Display for MemoryScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent => write!(f, "missing"),
            Self::Failed(message) => write!(f, "{message}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryStore {
    ws: Arc<dyn Workspace>,
}

impl MemoryStore {
    pub(crate) fn new(ws: Arc<dyn Workspace>) -> Self {
        Self { ws }
    }

    pub(crate) fn bot_memory(slug: &str) -> String {
        format!("{slug}/memory/MEMORY.md")
    }

    pub(crate) fn bot_log(slug: &str) -> String {
        format!("{slug}/memory/log.md")
    }

    pub(crate) fn shared_memory() -> &'static str {
        ".botapp/shared-memory.md"
    }

    pub(crate) async fn exists(&self, rel: &str) -> io::Result<bool> {
        self.ws.exists(rel).await
    }

    /// Existing Bot folder only. A symlink, a file, or a missing folder is [`MemoryScopeError::Absent`].
    pub(crate) async fn require_bot_dir(&self, slug: &str) -> Result<(), MemoryScopeError> {
        if !valid_slug(slug) {
            return Err(MemoryScopeError::Absent);
        }
        match self.ws.require_dir(slug).await {
            Ok(()) => Ok(()),
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
                ) =>
            {
                Err(MemoryScopeError::Absent)
            }
            Err(err) => Err(MemoryScopeError::Failed(err.to_string())),
        }
    }

    /// Create the Bot folder and `memory/` when they are missing.
    /// A symlink or a file at either key is an error.
    pub(crate) async fn ensure_bot_dir(&self, slug: &str) -> io::Result<()> {
        if !valid_slug(slug) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad slug"));
        }
        self.ws.ensure_dir(slug).await?;
        self.ws.ensure_dir(&format!("{slug}/memory")).await
    }

    /// The first `cap` bytes of a real file. A missing key, a directory, or any symlink
    /// in the path is an empty string.
    pub(crate) async fn read_capped(&self, rel: &str, cap: usize) -> io::Result<(String, bool)> {
        if !self.real_file(rel).await? {
            return Ok((String::new(), false));
        }
        let bytes = self.ws.read(rel).await?;
        Ok(cap_prefix(&bytes, cap))
    }

    pub(crate) async fn write_text(
        &self,
        rel: &str,
        text: &str,
        mode: Option<u32>,
    ) -> io::Result<()> {
        self.refuse_symlink(rel).await?;
        self.ws.write(rel, text.as_bytes(), mode).await
    }

    /// `Ok(true)` when the file was created. `Ok(false)` when a file is already there.
    /// A symlink is an error.
    pub(crate) async fn write_text_if_absent(
        &self,
        rel: &str,
        text: &str,
        mode: Option<u32>,
    ) -> io::Result<bool> {
        self.refuse_symlink(rel).await?;
        match self.ws.symlink_kind(rel).await? {
            None => {
                self.write_text(rel, text, mode).await?;
                Ok(true)
            }
            Some(EntryKind::File) => Ok(false),
            Some(EntryKind::Dir) => Err(io::Error::new(io::ErrorKind::InvalidInput, "directory")),
            Some(EntryKind::Symlink) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to write through a symlink",
            )),
        }
    }

    pub(crate) async fn skills(
        &self,
        slug: &str,
        cap: usize,
    ) -> io::Result<Vec<skills::SkillText>> {
        skills::read_skills(self, slug, cap).await
    }

    /// New Bot memory files. An existing shared file is left as it is.
    pub(crate) async fn bootstrap(&self, slug: &str, name: &str) -> io::Result<()> {
        self.ensure_bot_dir(slug).await?;
        self.ws.ensure_dir(&format!("{slug}/library")).await?;
        self.ws.ensure_dir(&format!("{slug}/skills")).await?;
        self.write_text_if_absent(
            &Self::bot_memory(slug),
            &format!("# {name} memory\n\n"),
            None,
        )
        .await?;
        self.write_text_if_absent(&Self::bot_log(slug), "# Work log\n\n", None)
            .await?;
        self.ws.ensure_dir(".botapp").await?;
        self.write_text_if_absent(Self::shared_memory(), "# Shared facts (all Bots)\n\n", None)
            .await?;
        Ok(())
    }

    async fn real_file(&self, rel: &str) -> io::Result<bool> {
        let parts: Vec<&str> = rel.split('/').collect();
        let mut built = String::new();
        for (index, part) in parts.iter().enumerate() {
            if !built.is_empty() {
                built.push('/');
            }
            built.push_str(part);
            let last = index + 1 == parts.len();
            match self.ws.symlink_kind(&built).await? {
                Some(EntryKind::Symlink) | None => return Ok(false),
                Some(EntryKind::Dir) if !last => {}
                Some(EntryKind::File) if last => return Ok(true),
                Some(EntryKind::Dir) | Some(EntryKind::File) => return Ok(false),
            }
        }
        Ok(false)
    }

    async fn refuse_symlink(&self, rel: &str) -> io::Result<()> {
        let mut built = String::new();
        for part in rel.split('/') {
            if part.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path escapes the workspace",
                ));
            }
            if !built.is_empty() {
                built.push('/');
            }
            built.push_str(part);
            if self.ws.symlink_kind(&built).await? == Some(EntryKind::Symlink) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "refusing to write through a symlink",
                ));
            }
        }
        Ok(())
    }
}

/// Keys later phases read. Phase 2 bootstraps and reads memory, the log, and skills.
#[allow(
    dead_code,
    reason = "later memory phases use these keys; this phase only bootstraps and reads"
)]
impl MemoryStore {
    pub(crate) fn bot_state(slug: &str) -> String {
        format!("{slug}/memory/.state.json")
    }

    pub(crate) fn bot_dreams(slug: &str) -> String {
        format!("{slug}/memory/DREAMS.md")
    }

    pub(crate) fn bot_history_dir(slug: &str) -> String {
        format!("{slug}/memory/history")
    }

    pub(crate) fn bot_journal_dir(slug: &str) -> String {
        format!("{slug}/memory/.journal")
    }

    pub(crate) fn bot_note(slug: &str, under_notes: &str) -> io::Result<String> {
        if !valid_slug(slug) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad slug"));
        }
        let rel = format!("{slug}/memory/notes/{under_notes}");
        crate::storage::workspace::safe_rel(&rel)?;
        Ok(rel)
    }

    pub(crate) fn shared_state() -> &'static str {
        ".botapp/shared-memory.state.json"
    }

    pub(crate) fn shared_history_dir() -> &'static str {
        ".botapp/shared-memory.history"
    }

    pub(crate) fn shared_journal_dir() -> &'static str {
        ".botapp/shared-memory.journal"
    }

    pub(crate) fn shared_retired(month: &str) -> String {
        format!(".botapp/shared-history/closed-{month}.md")
    }

    pub(crate) async fn read_text(&self, rel: &str) -> io::Result<String> {
        match self.ws.symlink_kind(rel).await? {
            Some(EntryKind::File) => {
                let bytes = self.ws.read(rel).await?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
            Some(EntryKind::Symlink) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to read through a symlink",
            )),
            Some(EntryKind::Dir) => Err(io::Error::new(io::ErrorKind::InvalidInput, "directory")),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "missing")),
        }
    }

    pub(crate) async fn read_text_or_empty(&self, rel: &str) -> io::Result<String> {
        match self.read_text(rel).await {
            Ok(text) => Ok(text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(err) => Err(err),
        }
    }
}

fn cap_prefix(bytes: &[u8], cap: usize) -> (String, bool) {
    if bytes.len() <= cap {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let mut buf = bytes[..cap].to_vec();
    while buf.last().is_some_and(|b| b & 0b1100_0000 == 0b1000_0000) {
        buf.pop();
    }
    if buf.last().is_some_and(|b| *b >= 0xC0) {
        buf.pop();
    }
    (String::from_utf8_lossy(&buf).into_owned(), true)
}
