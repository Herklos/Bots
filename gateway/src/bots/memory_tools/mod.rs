//! Read-only memory and skills on the Bots' own tool server (`POST /mcp/bot`).
//!
//! The caller is the Bot whose `mainThreadId` is `threadId`. A slug, path, or
//! `parentSlug` in the arguments is ignored. A subagent (`role` `subagent`, or
//! `hideFromSidebar`) also reads its parent's `memory/` and `skills/`. The parent
//! folder is `parentSlug` on the caller record. These tools never write, and they
//! do not follow a symlink or a `..` segment out of a Bot folder.
//!
//! Each file is capped at [`FILE_CAP`] bytes. A missing file is an empty string.
//! A missing parent folder is a tool error.

use crate::{bots::valid_slug, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

#[cfg(test)]
mod tests;

/// Bytes kept from each file. Longer files return `truncated: true`.
pub(crate) const FILE_CAP: usize = 16 * 1024;

const PARENT_MISSING: &str = "This subagent's parent folder is missing.";

const NAMES: [&str; 2] = ["memory_read", "skills_read"];

/// What the tools need, passed in so tests can supply a temp directory.
pub(crate) struct Deps<'a> {
    pub root: &'a Path,
    pub roster: &'a [Value],
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
            "description": format!("Read your memory/MEMORY.md and memory/log.md. A missing file is empty. A subagent also receives its parent's copies, read-only. {cap}"),
            "inputSchema": {
                "type": "object",
                "properties": { "threadId": thread },
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
}

pub(crate) fn is_memory_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Entry point from `mcp.rs`.
pub(crate) async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let roster = state.bots.list().await.map_err(|e| e.message)?;
    dispatch(
        &Deps {
            root: &state.cfg.bots_root,
            roster: &roster,
        },
        name,
        args,
    )
    .await
}

pub(crate) async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "memory_read" => read_memory(d, args).await,
        "skills_read" => read_skills(d, args).await,
        _ => Err(format!("Unknown tool {name}")),
    }
}

fn caller<'a>(roster: &'a [Value], args: &Value) -> Result<&'a Value, String> {
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

/// Read `rel` under `bot_dir`. A missing file, a directory, or a symlink is empty.
pub(crate) async fn read_rel(bot_dir: &Path, rel: &[&str]) -> Result<Value, String> {
    let mut path = bot_dir.to_path_buf();
    for part in rel {
        if part.is_empty()
            || *part == "."
            || *part == ".."
            || part.contains('/')
            || part.contains('\\')
        {
            return Ok(empty_file());
        }
        path.push(part);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(meta) if meta.file_type().is_symlink() => return Ok(empty_file()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(empty_file()),
            Err(e) => return Err(e.to_string()),
        }
    }
    let meta = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() {
        return Ok(empty_file());
    }
    let (text, truncated) = read_default(&path).await?;
    Ok(file_value(text, truncated))
}

async fn read_default(path: &Path) -> Result<(String, bool), String> {
    read_capped(path, FILE_CAP).await
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

fn skill_file_name(name: &str) -> bool {
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

async fn skills_in(bot_dir: &Path) -> Result<Vec<Value>, String> {
    let skills_dir = bot_dir.join("skills");
    let meta = match tokio::fs::symlink_metadata(&skills_dir).await {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.file_type().is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    let mut entries = tokio::fs::read_dir(&skills_dir)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if skill_file_name(&name) {
            names.push(name);
        }
    }
    names.sort();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let path = skills_dir.join(&name);
        let meta = match tokio::fs::symlink_metadata(&path).await {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let (text, truncated) = read_default(&path).await?;
        out.push(json!({ "name": name, "text": text, "truncated": truncated }));
    }
    Ok(out)
}

async fn memory_of(bot_dir: &Path) -> Result<Value, String> {
    Ok(json!({
        "MEMORY.md": read_rel(bot_dir, &["memory", "MEMORY.md"]).await?,
        "log.md": read_rel(bot_dir, &["memory", "log.md"]).await?,
    }))
}

async fn scope(d: &Deps<'_>, args: &Value) -> Result<(PathBuf, Option<PathBuf>), String> {
    let bot = caller(d.roster, args)?;
    let slug = bot["slug"].as_str().unwrap_or_default();
    let own = match folder(d.root, slug).await? {
        Folder::Ready(dir) => dir,
        Folder::Absent => PathBuf::new(),
    };
    let parent = if is_subagent(bot) {
        let parent_slug = parent_slug(bot).ok_or(PARENT_MISSING)?;
        match folder(d.root, parent_slug).await? {
            Folder::Ready(dir) => Some(dir),
            Folder::Absent => return Err(PARENT_MISSING.to_string()),
        }
    } else {
        None
    };
    Ok((own, parent))
}

async fn read_memory(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (own, parent) = scope(d, args).await?;
    let memory = if own.as_os_str().is_empty() {
        json!({
            "MEMORY.md": empty_file(),
            "log.md": empty_file(),
        })
    } else {
        memory_of(&own).await?
    };
    let mut result = json!({ "memory": memory });
    if let Some(parent) = parent {
        result["parent"] = json!({ "memory": memory_of(&parent).await? });
    }
    Ok(result)
}

async fn read_skills(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (own, parent) = scope(d, args).await?;
    let skills = if own.as_os_str().is_empty() {
        Vec::new()
    } else {
        skills_in(&own).await?
    };
    let mut result = json!({ "skills": skills });
    if let Some(parent) = parent {
        result["parent"] = json!({ "skills": skills_in(&parent).await? });
    }
    Ok(result)
}
