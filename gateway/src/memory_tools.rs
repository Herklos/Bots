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

/// Bytes kept from each file. Longer files return `truncated: true`.
pub const FILE_CAP: usize = 16 * 1024;

const PARENT_MISSING: &str = "This subagent's parent folder is missing.";
const NAMES: [&str; 2] = ["memory_read", "skills_read"];

/// What the tools need, passed in so tests can supply a temp directory.
pub struct Deps<'a> {
    pub root: &'a Path,
    pub roster: &'a [Value],
}

pub fn tool_defs() -> Vec<Value> {
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

pub fn is_memory_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Entry point from `mcp.rs`.
pub async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
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

pub async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
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

enum Folder {
    Ready(PathBuf),
    Absent,
}

async fn folder(root: &Path, slug: &str) -> Result<Folder, String> {
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
async fn read_rel(bot_dir: &Path, rel: &[&str]) -> Result<Value, String> {
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
    let (text, truncated) = read_capped(&path).await?;
    Ok(file_value(text, truncated))
}

async fn read_capped(path: &Path) -> Result<(String, bool), String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.take(u64::try_from(FILE_CAP).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    let truncated = buf.len() > FILE_CAP;
    if truncated {
        buf.truncate(FILE_CAP);
        // Drop a character that the cap split in half.
        while buf.last().is_some_and(|b| b & 0b1100_0000 == 0b1000_0000) {
            buf.pop();
        }
        if buf.last().is_some_and(|b| *b >= 0xC0) {
            buf.pop();
        }
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

fn skill_file_name(name: &str) -> bool {
    name.ends_with(".md")
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
        let (text, truncated) = read_capped(&path).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(slug: &str, thread: &str, role: &str, parent: Option<&str>, hide: bool) -> Value {
        json!({
            "slug": slug,
            "name": slug,
            "mainThreadId": thread,
            "role": role,
            "parentSlug": parent,
            "hideFromSidebar": hide,
        })
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn names_are_served_here() {
        assert!(is_memory_tool("memory_read"));
        assert!(is_memory_tool("skills_read"));
        assert!(!is_memory_tool("vault_list"));
        let defs = tool_defs();
        let names: Vec<&str> = defs.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for name in NAMES {
            assert!(names.contains(&name), "{name} is listed");
        }
        let described = tool_defs()
            .iter()
            .all(|t| t["description"].as_str().unwrap().contains("16384"));
        assert!(described);
    }

    #[tokio::test]
    async fn a_bot_reads_its_own_memory_and_skills() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join("ada/memory/MEMORY.md"),
            "ada remembers the gate code",
        );
        write(&root.join("ada/skills/open.md"), "turn the key");
        let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let args = json!({ "threadId": "thread-ada" });
        let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
        assert_eq!(
            memory["memory"]["MEMORY.md"]["text"],
            "ada remembers the gate code"
        );
        assert_eq!(memory["memory"]["log.md"]["text"], "");
        assert_eq!(memory["memory"]["log.md"]["truncated"], false);
        assert!(memory.get("parent").is_none());
        let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
        assert_eq!(skills["skills"][0]["name"], "open.md");
        assert_eq!(skills["skills"][0]["text"], "turn the key");
        assert!(skills.get("parent").is_none());
    }

    #[tokio::test]
    async fn a_subagent_reads_its_parents_memory_and_skills() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("ada/memory/MEMORY.md"), "parent fact");
        write(&root.join("ada/memory/log.md"), "parent log");
        write(&root.join("ada/skills/shared.md"), "parent skill");
        write(&root.join("ada-jr/memory/MEMORY.md"), "child fact");
        write(&root.join("ada-jr/skills/own.md"), "child skill");
        let roster = vec![
            bot("ada", "thread-ada", "bot", None, false),
            bot("ada-jr", "thread-jr", "subagent", Some("ada"), true),
        ];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let args = json!({ "threadId": "thread-jr", "slug": "ada", "parentSlug": "nope" });
        let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
        assert_eq!(memory["memory"]["MEMORY.md"]["text"], "child fact");
        assert_eq!(
            memory["parent"]["memory"]["MEMORY.md"]["text"],
            "parent fact"
        );
        assert_eq!(memory["parent"]["memory"]["log.md"]["text"], "parent log");
        let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
        assert_eq!(skills["skills"][0]["text"], "child skill");
        assert_eq!(skills["parent"]["skills"][0]["name"], "shared.md");
        assert_eq!(skills["parent"]["skills"][0]["text"], "parent skill");
    }

    #[tokio::test]
    async fn hide_from_sidebar_also_reads_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("ada/memory/log.md"), "from the parent");
        write(&root.join("ada-jr/memory/log.md"), "from the child");
        let roster = vec![bot("ada-jr", "thread-jr", "bot", Some("ada"), true)];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let memory = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-jr" }))
            .await
            .unwrap();
        assert_eq!(
            memory["parent"]["memory"]["log.md"]["text"],
            "from the parent"
        );
    }

    #[tokio::test]
    async fn a_normal_bot_cannot_read_another_bots_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("ada/memory/MEMORY.md"), "ada-secret");
        write(&root.join("ada/skills/ada.md"), "ada-skill");
        write(&root.join("bea/memory/MEMORY.md"), "bea-secret");
        write(&root.join("bea/skills/bea.md"), "bea-skill");
        let roster = vec![
            bot("ada", "thread-ada", "bot", Some("bea"), false),
            bot("bea", "thread-bea", "bot", None, false),
        ];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let memory = dispatch(
            &deps,
            "memory_read",
            &json!({ "threadId": "thread-ada", "slug": "bea", "path": "../bea/memory/MEMORY.md" }),
        )
        .await
        .unwrap();
        let dumped = memory.to_string();
        assert!(dumped.contains("ada-secret"));
        assert!(!dumped.contains("bea-secret"));
        assert!(memory.get("parent").is_none());
        let skills = dispatch(
            &deps,
            "skills_read",
            &json!({ "threadId": "thread-ada", "slug": "bea" }),
        )
        .await
        .unwrap();
        let dumped = skills.to_string();
        assert!(dumped.contains("ada-skill"));
        assert!(!dumped.contains("bea-skill"));
    }

    #[tokio::test]
    async fn a_forged_slug_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("ada/memory/MEMORY.md"), "only-ada");
        write(&root.join("bea/memory/MEMORY.md"), "only-bea");
        let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let memory = dispatch(
            &deps,
            "memory_read",
            &json!({
                "threadId": "thread-ada",
                "slug": "bea",
                "parentSlug": "bea",
                "path": "bea/memory/MEMORY.md",
            }),
        )
        .await
        .unwrap();
        assert_eq!(memory["memory"]["MEMORY.md"]["text"], "only-ada");
        assert!(!memory.to_string().contains("only-bea"));
    }

    #[tokio::test]
    async fn a_missing_parent_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("ada-jr/memory/MEMORY.md"), "child");
        let roster = vec![bot(
            "ada-jr",
            "thread-jr",
            "subagent",
            Some("missing"),
            true,
        )];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let err = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-jr" }))
            .await
            .unwrap_err();
        assert_eq!(err, PARENT_MISSING);
        let forged = dispatch(
            &deps,
            "skills_read",
            &json!({ "threadId": "thread-jr", "parentSlug": "ada" }),
        )
        .await
        .unwrap_err();
        assert_eq!(forged, PARENT_MISSING);
    }

    #[tokio::test]
    async fn a_parent_slug_that_escapes_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("ada-jr/memory/MEMORY.md"), "child");
        write(&dir.path().join("../outside.md"), "outside");
        let roster = vec![bot(
            "ada-jr",
            "thread-jr",
            "subagent",
            Some("../outside"),
            false,
        )];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let err = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-jr" }))
            .await
            .unwrap_err();
        assert_eq!(err, PARENT_MISSING);
        assert!(!err.contains("outside"));
    }

    #[tokio::test]
    async fn files_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let huge = "x".repeat(FILE_CAP + 40);
        write(&root.join("ada/memory/MEMORY.md"), &huge);
        write(&root.join("ada/skills/big.md"), &huge);
        let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let args = json!({ "threadId": "thread-ada" });
        let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
        let text = memory["memory"]["MEMORY.md"]["text"].as_str().unwrap();
        assert_eq!(text.len(), FILE_CAP);
        assert_eq!(memory["memory"]["MEMORY.md"]["truncated"], true);
        assert_ne!(text, huge);
        let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
        assert_eq!(
            skills["skills"][0]["text"].as_str().unwrap().len(),
            FILE_CAP
        );
        assert_eq!(skills["skills"][0]["truncated"], true);
    }

    #[tokio::test]
    async fn a_symlink_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("bea/memory/MEMORY.md"), "bea-secret");
        write(&root.join("bea/skills/hidden.md"), "bea-skill");
        std::fs::create_dir_all(root.join("ada/memory")).unwrap();
        std::fs::create_dir_all(root.join("ada/skills")).unwrap();
        std::os::unix::fs::symlink(
            root.join("bea/memory/MEMORY.md"),
            root.join("ada/memory/MEMORY.md"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            root.join("bea/skills/hidden.md"),
            root.join("ada/skills/link.md"),
        )
        .unwrap();
        write(&root.join("ada/skills/real.md"), "ada-skill");
        let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
        let deps = Deps {
            root,
            roster: &roster,
        };
        let args = json!({ "threadId": "thread-ada" });
        let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
        assert_eq!(memory["memory"]["MEMORY.md"]["text"], "");
        assert!(!memory.to_string().contains("bea-secret"));
        let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
        assert_eq!(skills["skills"].as_array().unwrap().len(), 1);
        assert_eq!(skills["skills"][0]["name"], "real.md");
        assert!(!skills.to_string().contains("bea-skill"));
    }
}
