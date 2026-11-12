//! Plain Markdown the person edits: `memory/MEMORY.md` and `.botapp/shared-memory.md`.
//!
//! A save carries the revision it loaded. That revision is the sha256 of the
//! file with CR stripped, so an edit a Bot made in between is not overwritten.
//! Saved text is normalized, and managed-block markers are removed, before it is written.

use crate::bots::agent_context::plain_prompt;
use crate::bots::memory::{revision_of as content_revision, MemoryStore};
use crate::bots::{valid_slug, MANAGED_END, MANAGED_START};
use std::io;

/// A Memory tab save larger than this is refused. The AGENTS.md snapshot stays smaller.
pub(crate) const FILE_CHARS: usize = 200_000;

const MARKER_REMOVED: &str = "[managed marker removed]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileScope {
    Bot,
    Shared,
}

impl FileScope {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "bot" => Some(Self::Bot),
            "shared" => Some(Self::Shared),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::Shared => "shared",
        }
    }

    pub(crate) fn path(self) -> &'static str {
        match self {
            Self::Bot => "memory/MEMORY.md",
            Self::Shared => ".botapp/shared-memory.md",
        }
    }

    fn rel(self, slug: &str) -> String {
        match self {
            Self::Bot => MemoryStore::bot_memory(slug),
            Self::Shared => MemoryStore::shared_memory().to_string(),
        }
    }

    /// Same keys as fact mutations, so the two writers cannot pass each other.
    fn lock_key(self, slug: &str) -> String {
        match self {
            Self::Bot => format!("bot:{slug}"),
            Self::Shared => "shared".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryFile {
    pub scope: FileScope,
    pub path: &'static str,
    pub text: String,
    pub revision: String,
    /// The editor reads the whole file. The snapshot clip is a later preview.
    pub truncated: bool,
}

#[derive(Debug)]
pub(crate) enum SaveFail {
    Invalid(String),
    Conflict(MemoryFile),
    Io(io::Error),
}

pub(crate) fn revision_of(content: &str) -> String {
    content_revision(content)
}

/// Line feeds only, no control bytes, and no markers that would close the managed block.
pub(crate) fn sanitize_memory_text(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let stripped: String = normalized
        .chars()
        .filter(|c| {
            let code = u32::from(*c);
            !((code <= 0x08)
                || code == 0x0b
                || code == 0x0c
                || (0x0e..=0x1f).contains(&code)
                || code == 0x7f)
        })
        .collect();
    let marked = stripped
        .replace(MANAGED_START, MARKER_REMOVED)
        .replace(MANAGED_END, MARKER_REMOVED);
    plain_prompt(&marked)
}

pub(crate) async fn read_file(
    store: &MemoryStore,
    scope: FileScope,
    slug: &str,
) -> io::Result<MemoryFile> {
    if let FileScope::Bot = scope {
        if !valid_slug(slug) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad slug"));
        }
    }
    let text = store.read_text_or_empty(&scope.rel(slug)).await?;
    Ok(MemoryFile {
        scope,
        path: scope.path(),
        revision: revision_of(&text),
        text,
        truncated: false,
    })
}

pub(crate) async fn save_file(
    store: &MemoryStore,
    scope: FileScope,
    slug: &str,
    text: &str,
    expected_revision: &str,
) -> Result<MemoryFile, SaveFail> {
    if let FileScope::Bot = scope {
        if !valid_slug(slug) {
            return Err(SaveFail::Invalid("bad slug".to_string()));
        }
    }
    if text.chars().count() > FILE_CHARS {
        return Err(too_large());
    }
    let _guard = store.lock_file(&scope.lock_key(slug)).await;
    let current = read_file(store, scope, slug).await.map_err(SaveFail::Io)?;
    if current.revision != expected_revision {
        return Err(SaveFail::Conflict(current));
    }
    let finished = with_trailing_newline(sanitize_memory_text(text));
    if finished.chars().count() > FILE_CHARS + 1 {
        return Err(too_large());
    }
    store
        .write_text(&scope.rel(slug), &finished, Some(0o644))
        .await
        .map_err(SaveFail::Io)?;
    read_file(store, scope, slug).await.map_err(SaveFail::Io)
}

fn with_trailing_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn too_large() -> SaveFail {
    SaveFail::Invalid(format!("Memory is limited to {FILE_CHARS} characters."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_drops_markers_controls_and_memory_tags() {
        let raw = format!(
            "Likes tea\r\n{MANAGED_START}\n{MANAGED_END}\n</memory>\n<memory>\na\u{0000}b\tc"
        );
        let clean = sanitize_memory_text(&raw);
        assert!(!clean.contains(MANAGED_START));
        assert!(!clean.contains(MANAGED_END));
        assert!(!clean.contains("</memory>"));
        assert!(!clean.contains("<memory>"));
        assert!(!clean.contains('\r'));
        assert!(!clean.contains('\u{0000}'));
        assert!(clean.contains("Likes tea\n"));
        assert!(clean.contains(MARKER_REMOVED));
        assert!(clean.contains("&lt;/memory>"));
        assert!(clean.contains("&lt;memory>"));
        assert!(clean.contains("ab\tc"));
    }

    #[test]
    fn revision_of_empty_matches_the_empty_digest() {
        assert_eq!(
            revision_of(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(revision_of("a\r\nb"), revision_of("a\nb"));
        assert_ne!(revision_of("a"), revision_of("b"));
    }
}
