//! `image_show` on the Bots' own tool server (`POST /mcp/bot`).
//!
//! The caller is the Bot whose `mainThreadId` is `threadId`. A slug in the
//! arguments is ignored. A subagent writes only its own `library/`.
//! A symlink, a file over 5 MiB, or a file that is not a PNG, JPEG, GIF or WebP
//! is refused. A picture saved outside `library/` is copied in with a name that
//! does not overwrite.

use crate::chat::images::{kind_of, picture, read_regular, ImageFault, MAX_IMAGE_BYTES};
use crate::storage::workspace::{EntryKind, Workspace};
use crate::{bots::valid_slug, AppState};
use serde_json::{json, Value};

const OUTSIDE: &str = "That path is not in this Bot's folder.";
const MISSING: &str = "That file is missing.";
const NOT_FILE: &str = "That path is not a file.";
const UNAVAILABLE: &str = "That path is not available.";
const SYMLINK: &str = "is a symlink";
const TOO_BIG: &str = "larger than 5 MiB";
const NOT_IMAGE: &str = "not an image";
const UNSUPPORTED: &str = "unsupported type";

/// What the tool needs. Tests pass a workspace and a roster.
pub(crate) struct Deps<'a> {
    pub workspace: &'a dyn Workspace,
    pub roster: &'a [Value],
}

pub(crate) fn tool_defs() -> Vec<Value> {
    let thread = json!({
        "type": "string",
        "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/bot.json)."
    });
    vec![json!({
        "name": "image_show",
        "description": "Check a picture and return the Markdown to put in your reply. path is a PNG, JPEG, GIF or WebP file in your folder, not under a hidden directory or memory/. A file outside library/ is copied there with a name that does not overwrite. At most 5 MiB. A symlink is refused.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "threadId": thread,
                "path": { "type": "string", "description": "A file in your folder, such as scratch/chart.png or library/chart.png." },
                "alt": { "type": "string", "description": "Optional caption. The default is the file name." }
            },
            "required": ["threadId", "path"],
        },
    })]
}

pub(crate) fn is_image_tool(name: &str) -> bool {
    name == "image_show"
}

/// Entry point from `mcp.rs`.
pub(crate) async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let roster = state.bots.list().await.map_err(|err| err.message)?;
    dispatch(
        &Deps {
            workspace: state.workspace.as_ref(),
            roster: &roster,
        },
        name,
        args,
    )
    .await
}

pub(crate) async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "image_show" => show(d, args).await,
        _ => Err(format!("Unknown tool {name}")),
    }
}

fn caller<'a>(roster: &'a [Value], args: &Value) -> Result<&'a Value, String> {
    let thread = args["threadId"]
        .as_str()
        .map(str::trim)
        .filter(|thread| !thread.is_empty())
        .ok_or("threadId is required: pass your main thread id from AGENTS.md.")?;
    roster.iter().find(|bot| bot["mainThreadId"] == thread).ok_or_else(|| {
        "That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/bot.json).".to_string()
    })
}

async fn show(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let bot = caller(d.roster, args)?;
    let slug = bot["slug"].as_str().unwrap_or("");
    if !valid_slug(slug) {
        return Err("That Bot has no folder.".into());
    }
    let root = bot["workspaceRoot"].as_str().unwrap_or("");
    let raw = args["path"]
        .as_str()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or("path is required.")?;
    let rel = normalize(raw, root)?;
    let kind = kind_of(&rel).ok_or_else(|| UNSUPPORTED.to_string())?;
    require_file(d.workspace, slug, &rel).await?;
    let bytes = read_regular(d.workspace, &format!("{slug}/{rel}"))
        .await
        .map_err(fault_text)?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(TOO_BIG.into());
    }
    let seen = picture(kind, &bytes).ok_or_else(|| NOT_IMAGE.to_string())?;
    let library_rel = if rel.starts_with("library/") {
        rel
    } else {
        copy_into_library(d.workspace, slug, &rel, &bytes).await?
    };
    let alt = alt_of(args, &library_rel);
    Ok(json!({
        "markdown": markdown(&alt, &library_rel),
        "bytes": bytes.len(),
        "mime": seen.mime,
        "width": seen.width,
        "height": seen.height,
    }))
}

fn fault_text(fault: ImageFault) -> String {
    match fault {
        ImageFault::Missing => MISSING.into(),
        ImageFault::Link => SYMLINK.into(),
        ImageFault::TooBig => TOO_BIG.into(),
        ImageFault::NotImage => NOT_IMAGE.into(),
        ImageFault::BadPath => OUTSIDE.into(),
        ImageFault::Io => "The picture could not be read.".into(),
    }
}

fn normalize(raw: &str, workspace_root: &str) -> Result<String, String> {
    let trimmed = unwrap_once(raw.trim()).trim();
    if trimmed.is_empty() || forbidden(trimmed) {
        return Err(OUTSIDE.into());
    }
    let decoded = percent_decode(trimmed).ok_or_else(|| OUTSIDE.to_string())?;
    if forbidden(&decoded) {
        return Err(OUTSIDE.into());
    }
    let rel = if let Some(rest) = decoded.strip_prefix("./") {
        rest.to_string()
    } else if decoded.starts_with('/') {
        under_root(&decoded, workspace_root)?
    } else {
        decoded
    };
    check_segments(&rel)?;
    Ok(rel)
}

fn unwrap_once(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let (open, close) = (bytes[0], bytes[bytes.len() - 1]);
        let wrapped = (open == b'<' && close == b'>')
            || (open == b'"' && close == b'"')
            || (open == b'\'' && close == b'\'');
        if wrapped {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn under_root(path: &str, root: &str) -> Result<String, String> {
    let root = root.trim().trim_end_matches('/');
    if root.is_empty() {
        return Err(OUTSIDE.into());
    }
    let rest = path.strip_prefix(root).ok_or_else(|| OUTSIDE.to_string())?;
    let rest = rest.strip_prefix('/').ok_or_else(|| OUTSIDE.to_string())?;
    if rest.is_empty() {
        return Err(OUTSIDE.into());
    }
    Ok(rest.to_string())
}

fn forbidden(value: &str) -> bool {
    value.starts_with("//")
        || value.contains('\\')
        || value.chars().any(|ch| ch == '\0' || ch.is_control())
        || has_scheme(value)
}

fn has_scheme(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["http:", "https:", "data:", "file:", "javascript:"]
        .iter()
        .any(|scheme| {
            lower
                .match_indices(scheme)
                .any(|(at, _)| at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric())
        })
}

fn check_segments(rel: &str) -> Result<(), String> {
    if rel.is_empty() || rel.starts_with('/') || rel.ends_with('/') {
        return Err(OUTSIDE.into());
    }
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(['?', '#']) {
            return Err(OUTSIDE.into());
        }
        if part.starts_with('.') || part == "memory" {
            return Err(UNAVAILABLE.into());
        }
    }
    Ok(())
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = hex_val(*bytes.get(index + 1)?)?;
            let lo = hex_val(*bytes.get(index + 2)?)?;
            out.push((hi << 4) | lo);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

async fn kind_at(ws: &dyn Workspace, rel: &str) -> Result<Option<EntryKind>, String> {
    match ws.symlink_kind(rel).await {
        Ok(kind) => Ok(kind),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => Err(OUTSIDE.into()),
        Err(err) => Err(err.to_string()),
    }
}

async fn require_file(ws: &dyn Workspace, slug: &str, rel: &str) -> Result<(), String> {
    let mut acc = String::new();
    let parts: Vec<&str> = std::iter::once(slug).chain(rel.split('/')).collect();
    for (index, part) in parts.iter().enumerate() {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(part);
        let last = index + 1 == parts.len();
        match kind_at(ws, &acc).await? {
            Some(EntryKind::Symlink) => return Err(SYMLINK.into()),
            Some(EntryKind::Dir) if !last => {}
            Some(EntryKind::File) if last => {}
            Some(EntryKind::File | EntryKind::Dir) => return Err(NOT_FILE.into()),
            None => return Err(MISSING.into()),
        }
    }
    Ok(())
}

async fn copy_into_library(
    ws: &dyn Workspace,
    slug: &str,
    rel: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let name = rel.rsplit('/').next().ok_or(NOT_FILE)?;
    ensure_library(ws, slug).await?;
    let dest_name = free_name(ws, slug, name).await?;
    let dest = format!("library/{dest_name}");
    ws.write(&format!("{slug}/{dest}"), bytes, None)
        .await
        .map_err(|err| err.to_string())?;
    Ok(dest)
}

async fn ensure_library(ws: &dyn Workspace, slug: &str) -> Result<(), String> {
    let dir = format!("{slug}/library");
    match kind_at(ws, &dir).await? {
        Some(EntryKind::Symlink) => return Err(SYMLINK.into()),
        Some(EntryKind::File) => return Err("library/ is not a folder.".into()),
        Some(EntryKind::Dir) | None => {}
    }
    ws.ensure_dir(&dir).await.map_err(|err| err.to_string())
}

async fn free_name(ws: &dyn Workspace, slug: &str, name: &str) -> Result<String, String> {
    let (stem, ext) = name.rsplit_once('.').ok_or(UNSUPPORTED)?;
    for n in 0..1000u32 {
        let candidate = if n == 0 {
            name.to_string()
        } else {
            format!("{stem}-{suffix}.{ext}", suffix = n + 1)
        };
        match kind_at(ws, &format!("{slug}/library/{candidate}")).await? {
            None => return Ok(candidate),
            Some(EntryKind::File | EntryKind::Dir | EntryKind::Symlink) => {}
        }
    }
    Err("library/ has no free name for that picture.".into())
}

fn alt_of(args: &Value, library_rel: &str) -> String {
    let given = args["alt"].as_str().unwrap_or("").trim();
    let raw = if given.is_empty() {
        library_rel
            .rsplit('/')
            .next()
            .unwrap_or("picture")
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or("picture")
    } else {
        given
    };
    let cleaned: String = raw
        .chars()
        .map(|ch| {
            if ch == ']' || ch == '\n' || ch == '\r' || ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        "picture".to_string()
    } else {
        cleaned
    }
}

fn markdown(alt: &str, path: &str) -> String {
    if path.contains([' ', ')', '(']) {
        format!("![{alt}](<{path}>)")
    } else {
        format!("![{alt}]({path})")
    }
}
