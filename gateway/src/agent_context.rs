//! Text embedded in a Bot's managed `AGENTS.md` block.
//!
//! There is no per-thread instructions field on `orchestration.launchThread` or
//! `message.dispatch`, so this file is the channel a Bot's prompt uses.
use crate::bots::{MANAGED_END, MANAGED_START};
use crate::error::{ApiError, ApiResult};

pub(crate) const PROMPT_MAX: usize = 16 * 1024;

/// Caps for a later memory snapshot. Unused until that snapshot is rendered.
#[allow(
    dead_code,
    reason = "the memory snapshot that uses this cap is a later change"
)]
pub(crate) const MEMORY_MAX: usize = 16 * 1024;
#[allow(
    dead_code,
    reason = "the memory snapshot that uses this cap is a later change"
)]
pub(crate) const MEMORY_LOG_MAX: usize = 8 * 1024;
#[allow(
    dead_code,
    reason = "the memory snapshot that uses this cap is a later change"
)]
pub(crate) const SHARED_MEMORY_MAX: usize = 8 * 1024;
#[allow(
    dead_code,
    reason = "the memory snapshot that uses this cap is a later change"
)]
pub(crate) const PARENT_MEMORY_MAX: usize = 8 * 1024;

/// Placeholder for the next change, which fills the managed block from a snapshot.
/// The default is empty and renders nothing.
#[derive(Debug, Clone, Default)]
pub(crate) struct MemorySnapshot {
    memory: String,
    log: String,
    shared: String,
    parent: String,
}

impl MemorySnapshot {
    pub(crate) fn is_empty(&self) -> bool {
        [
            self.memory.as_str(),
            self.log.as_str(),
            self.shared.as_str(),
            self.parent.as_str(),
        ]
        .iter()
        .all(|text| text.trim().is_empty())
    }
}

const TAG_PREFIXES: &[&str] = &[
    "instructions",
    "memory-log",
    "shared-memory",
    "parent-memory",
    "memory",
];

pub(crate) fn ensure_prompt_limit(text: &str) -> ApiResult<()> {
    if text.len() > PROMPT_MAX {
        return Err(ApiError::bad_request("prompt is too long (max 16 KiB)"));
    }
    Ok(())
}

/// Strip the managed-block markers, then neutralise instruction and memory tags
/// so a prompt cannot close the block or open a section the file owns.
pub(crate) fn plain_prompt(text: &str) -> String {
    let text = text.replace(MANAGED_START, "").replace(MANAGED_END, "");
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && tag_starts_at(&chars, i) {
            out.push_str("&lt;");
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn tag_starts_at(chars: &[char], start: usize) -> bool {
    let mut j = start + 1;
    if chars.get(j) == Some(&'/') {
        j += 1;
    }
    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
        j += 1;
    }
    let rest: String = chars[j..].iter().collect::<String>().to_ascii_lowercase();
    TAG_PREFIXES.iter().any(|prefix| rest.starts_with(prefix))
}

pub(crate) fn prompt_section(heading: &str, raw: &str) -> String {
    let text = plain_prompt(raw).trim().to_string();
    if text.is_empty() {
        String::new()
    } else {
        format!("\n## {heading}\n\n{text}\n")
    }
}
