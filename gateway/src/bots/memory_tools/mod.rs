//! Memory and skills on the Bots' own tool server (`POST /mcp/bot`).
//!
//! The caller is the Bot whose `mainThreadId` is `threadId`. A slug, path, or
//! `parentSlug` in the arguments is ignored. A subagent (`role` `subagent`, or
//! `hideFromSidebar`) also reads its parent's `memory/` and `skills/`. The parent
//! folder is `parentSlug` on the caller record. A subagent writes only its own
//! store. None of these tools follow a symlink or a `..` segment out of a Bot folder.
//!
//! Each file is capped at [`FILE_CAP`] bytes. A missing file is an empty string.
//! A missing parent folder is a tool error.

use crate::bots::memory::{MemoryScopeError, MemoryStore};
use crate::{bots::valid_slug, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

mod edit;
mod read;
mod reflect;
mod suggest;

#[cfg(test)]
mod tests;

/// Bytes kept from each file. Longer files return `truncated: true`.
pub(crate) const FILE_CAP: usize = 16 * 1024;

const PARENT_MISSING: &str = "This subagent's parent folder is missing.";

const NAMES: [&str; 8] = [
    "memory_read",
    "memory_edit",
    "memory_forget",
    "memory_search",
    "memory_suggest",
    "memory_reflect",
    "memory_reflect_apply",
    "skills_read",
];

/// What the tools need. Tests pass a store and, for a write, the turn items.
pub(crate) struct Deps<'a> {
    pub memory: &'a MemoryStore,
    pub roster: &'a [Value],
    pub turn_items: &'a [Value],
    /// Slugs whose memory changed. `call` refreshes their agents afterwards.
    pub refreshed: Option<&'a std::sync::Mutex<Vec<String>>>,
    /// Bots root, so a write can see person-owned facts outside the Bot folder.
    pub bots_root: Option<&'a Path>,
}

impl<'a> Deps<'a> {
    #[cfg(test)]
    fn reading(memory: &'a MemoryStore, roster: &'a [Value]) -> Self {
        Self {
            memory,
            roster,
            turn_items: &[],
            refreshed: None,
            bots_root: None,
        }
    }
}

pub(crate) fn tool_defs() -> Vec<Value> {
    let thread = json!({
        "type": "string",
        "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/bot.json)."
    });
    let cap = format!(
        "Each file is at most {FILE_CAP} bytes; a longer file comes back with truncated set to true."
    );
    vec![
        json!({
            "name": "memory_read",
            "description": format!("Read your memory/MEMORY.md and memory/log.md, plus shared memory, a notes index, and the revision a later write must send back. Pass topic to read one note. Pass scope bot, shared, or all. A missing file is empty. A subagent also receives its parent's copies, read-only. {cap}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "threadId": thread,
                    "scope": { "type": "string", "enum": ["bot", "shared", "all"], "description": "What to read. The default is all." },
                    "topic": { "type": "string", "description": "A note path from the notes index, such as memory/notes/projects/launch.md." },
                    "full": { "type": "boolean", "description": "Read the whole note instead of the session-sized preview." }
                },
                "required": ["threadId"],
            },
            "annotations": { "readOnlyHint": true }
        }),
        json!({
            "name": "skills_read",
            "description": format!("Read your skills/*.md files: each name and its text. A missing skills folder is an empty list. A subagent also receives its parent's skills, read-only. {cap}"),
            "inputSchema": {
                "type": "object",
                "properties": { "threadId": thread },
                "required": ["threadId"],
            },
            "annotations": { "readOnlyHint": true }
        }),
    ]
    .into_iter()
    .chain(edit::defs(&thread))
    .chain(suggest::defs(&thread))
    .chain(reflect::defs(&thread))
    .collect()
}

pub(crate) fn is_memory_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Entry point from `mcp.rs`.
pub(crate) async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let roster = state.bots.list().await.map_err(|e| e.message)?;
    let items = if edit::needs_evidence(name) || reflect::needs_projection(name) {
        load_items(state, args).await?
    } else {
        Vec::new()
    };
    let refreshed = std::sync::Mutex::new(Vec::new());
    let result = dispatch(
        &Deps {
            memory: &state.memory,
            roster: &roster,
            turn_items: &items,
            refreshed: Some(&refreshed),
            bots_root: Some(&state.cfg.bots_root),
        },
        name,
        args,
    )
    .await;
    let slugs = refreshed
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone();
    for slug in slugs {
        if let Err(err) = state.bots.refresh_agents(&slug).await {
            tracing::warn!(slug, "memory saved; agents refresh failed: {}", err.message);
        }
    }
    result
}

async fn load_items(state: &AppState, args: &Value) -> Result<Vec<Value>, String> {
    let Some(thread) = args["threadId"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(Vec::new());
    };
    let transcript = state.harness.transcript().map_err(|err| err.to_string())?;
    let snapshot = transcript
        .read(thread)
        .await
        .map_err(|err| err.to_string())?;
    Ok(snapshot.items())
}

pub(crate) async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "memory_read" => read::read(d, args).await,
        "memory_edit" => edit::edit(d, args).await,
        "memory_forget" => edit::forget(d, args).await,
        "memory_search" => edit::search(d, args).await,
        "memory_suggest" => suggest::suggest(d, args).await,
        "memory_reflect" => reflect::reflect(d, args).await,
        "memory_reflect_apply" => reflect::apply(d, args).await,
        "skills_read" => read_skills(d, args).await,
        _ => Err(format!("Unknown tool {name}")),
    }
}

pub(super) fn caller<'a>(roster: &'a [Value], args: &Value) -> Result<&'a Value, String> {
    let thread = args["threadId"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("threadId is required: pass your main thread id from AGENTS.md.")?;
    roster.iter().find(|b| b["mainThreadId"] == thread).ok_or_else(|| {
        "That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/bot.json).".to_string()
    })
}

fn is_subagent(bot: &Value) -> bool {
    bot["role"] == "subagent" || bot["hideFromSidebar"] == true
}

fn parent_slug(bot: &Value) -> Option<&str> {
    bot.get("parentSlug")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub(crate) enum Folder {
    Ready(PathBuf),
    Absent,
}

pub(crate) async fn folder(root: &Path, slug: &str) -> Result<Folder, String> {
    if !valid_slug(slug) {
        return Ok(Folder::Absent);
    }
    let dir = root.join(slug);
    let meta = match tokio::fs::symlink_metadata(&dir).await {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Folder::Absent),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.file_type().is_dir() {
        return Ok(Folder::Absent);
    }
    let root_real = tokio::fs::canonicalize(root)
        .await
        .map_err(|e| e.to_string())?;
    let dir_real = tokio::fs::canonicalize(&dir)
        .await
        .map_err(|e| e.to_string())?;
    if !dir_real.starts_with(&root_real) {
        return Ok(Folder::Absent);
    }
    Ok(Folder::Ready(dir))
}

fn empty_file() -> Value {
    json!({ "text": "", "truncated": false })
}

fn file_value(text: String, truncated: bool) -> Value {
    json!({ "text": text, "truncated": truncated })
}

/// Read at most `cap` bytes from the start. A split UTF-8 character is dropped.
pub(crate) async fn read_capped(path: &Path, cap: usize) -> Result<(String, bool), String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.take(u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    let truncated = buf.len() > cap;
    if truncated {
        buf.truncate(cap);
        trim_partial_char_end(&mut buf);
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

/// Read at most `cap` bytes from the end. A split UTF-8 character at the start is dropped.
pub(crate) async fn read_tail(path: &Path, cap: usize) -> Result<(String, bool), String> {
    use tokio::io::AsyncSeekExt;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let len = file.metadata().await.map_err(|e| e.to_string())?.len();
    let cap_u64 = u64::try_from(cap).unwrap_or(u64::MAX);
    if len <= cap_u64 {
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        return Ok((String::from_utf8_lossy(&buf).into_owned(), false));
    }
    let start = len - cap_u64;
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    let mut i = 0;
    while i < buf.len() && buf[i] & 0b1100_0000 == 0b1000_0000 {
        i += 1;
    }
    Ok((String::from_utf8_lossy(&buf[i..]).into_owned(), true))
}

fn trim_partial_char_end(buf: &mut Vec<u8>) {
    while buf.last().is_some_and(|b| b & 0b1100_0000 == 0b1000_0000) {
        buf.pop();
    }
    if buf.last().is_some_and(|b| *b >= 0xC0) {
        buf.pop();
    }
}

pub(super) async fn memory_of(memory: &MemoryStore, slug: &str) -> Result<Value, String> {
    if !present(memory, slug).await? {
        return Ok(json!({
            "MEMORY.md": empty_file(),
            "log.md": empty_file(),
        }));
    }
    let (memory_text, memory_truncated) = memory
        .read_capped(&MemoryStore::bot_memory(slug), FILE_CAP)
        .await
        .map_err(|e| e.to_string())?;
    let (log_text, log_truncated) = memory
        .read_capped(&MemoryStore::bot_log(slug), FILE_CAP)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "MEMORY.md": file_value(memory_text, memory_truncated),
        "log.md": file_value(log_text, log_truncated),
    }))
}

async fn skills_of(memory: &MemoryStore, slug: &str) -> Result<Vec<Value>, String> {
    if !present(memory, slug).await? {
        return Ok(Vec::new());
    }
    let files = memory
        .skills(slug, FILE_CAP)
        .await
        .map_err(|e| e.to_string())?;
    Ok(files
        .into_iter()
        .map(|skill| {
            json!({
                "name": skill.name,
                "text": skill.text,
                "truncated": skill.truncated,
            })
        })
        .collect())
}

async fn present(memory: &MemoryStore, slug: &str) -> Result<bool, String> {
    match memory.require_bot_dir(slug).await {
        Ok(()) => Ok(true),
        Err(MemoryScopeError::Absent) => Ok(false),
        Err(MemoryScopeError::Failed(message)) => Err(message),
    }
}

pub(super) async fn scope(d: &Deps<'_>, args: &Value) -> Result<(String, Option<String>), String> {
    let bot = caller(d.roster, args)?;
    let slug = bot["slug"].as_str().unwrap_or_default().to_string();
    let parent = if is_subagent(bot) {
        let parent_slug = parent_slug(bot).ok_or(PARENT_MISSING)?;
        if present(d.memory, parent_slug).await? {
            Some(parent_slug.to_string())
        } else {
            return Err(PARENT_MISSING.to_string());
        }
    } else {
        None
    };
    Ok((slug, parent))
}

async fn read_skills(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (own, parent) = scope(d, args).await?;
    let mut result = json!({ "skills": skills_of(d.memory, &own).await? });
    if let Some(parent) = parent {
        result["parent"] = json!({ "skills": skills_of(d.memory, &parent).await? });
    }
    Ok(result)
}
