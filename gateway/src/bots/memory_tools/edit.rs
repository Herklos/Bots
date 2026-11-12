//! `memory_edit`, `memory_forget`, and `memory_search`.
use super::{caller, scope, Deps};
use crate::bots::memory::{
    check_owner_evidence, generic_attachment_quote, mutate, owner_messages, pick_message,
    require_unreviewed, search_memory, Coded, Hit, MutateFail, MutateOk, Mutation, Op, Scope,
};
use serde_json::{json, Value};

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;

pub(super) fn defs(thread: &Value) -> Vec<Value> {
    let scope = json!({
        "type": "string",
        "enum": ["bot", "shared"],
        "description": "bot (this Bot's memory, default) or shared (facts every Bot needs)."
    });
    let quote = json!({
        "type": "string",
        "description": "The user's exact typed phrase that supports the change."
    });
    vec![
        json!({
            "name": "memory_edit",
            "description": "Save one lasting fact, decision or preference to this Bot's memory or the shared memory, or write a note. Only the user's own typed words count as evidence: put the exact phrase in sourceQuote. If you have no words the person typed, use memory_suggest. Never save passwords, keys, tokens or identity numbers; use the Vault. expectedRevision must be the revision from memory_read (or an empty string to create a new note). If memory changed, the tool returns a conflict with the current content; read it, merge and retry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "op": { "type": "string", "enum": ["add", "replace", "supersede", "note"], "description": "add a fact, replace an existing one, supersede it with a dated new one, or write a note." },
                    "fact": { "type": "string", "description": "The fact for add, one line, 200 characters or fewer." },
                    "old": { "type": "string", "description": "The exact existing line for replace or supersede." },
                    "new": { "type": "string", "description": "The new fact for replace or supersede." },
                    "category": { "type": "string", "description": "Optional: preference, decision, commitment, person, project, lesson, fact, risk, question." },
                    "validFrom": { "type": "string", "description": "When this becomes true (YYYY-MM-DD). Required for supersede, optional for add." },
                    "restore": { "type": "boolean", "description": "Only when the user explicitly asks to remember a fact they previously forgot." },
                    "path": { "type": "string", "description": "For a note: under memory/notes and ending in .md." },
                    "description": { "type": "string", "description": "For a note: 1 to 160 characters shown in the notes index." },
                    "content": { "type": "string", "description": "For a note: the Markdown body. Frontmatter is added if missing." },
                    "scope": scope,
                    "expectedRevision": { "type": "string", "description": "The revision from memory_read. An empty string only creates a new note." },
                    "sourceMessageId": { "type": "string", "description": "The message id the quote came from. Omit it to use the latest user message." },
                    "sourceQuote": quote,
                    "threadId": thread
                },
                "required": ["op", "expectedRevision", "sourceQuote", "threadId"]
            },
            "annotations": { "destructiveHint": true }
        }),
        json!({
            "name": "memory_forget",
            "description": "Remove one exact fact from memory when the user asks to forget it. This writes a tombstone so the fact cannot come back on its own. It does not delete the conversation. Pass the exact line and the user's request in sourceQuote.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "fact": { "type": "string", "description": "The exact existing line to remove." },
                    "scope": scope,
                    "expectedRevision": { "type": "string", "description": "The revision from memory_read." },
                    "sourceMessageId": { "type": "string", "description": "The message id the request came from. Omit it to use the latest user message." },
                    "sourceQuote": quote,
                    "threadId": thread
                },
                "required": ["fact", "expectedRevision", "sourceQuote", "threadId"]
            },
            "annotations": { "destructiveHint": true }
        }),
        json!({
            "name": "memory_search",
            "description": "Search MEMORY.md, shared memory and notes. Use it before answering questions about past preferences, decisions or project details that are not already in view. Exact phrases score highest. Read one result with memory_read.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words or a phrase to find." },
                    "scope": { "type": "string", "enum": ["bot", "shared", "all"], "description": "Where to search. The default is all." },
                    "threadId": thread
                },
                "required": ["query", "threadId"]
            },
            "annotations": { "readOnlyHint": true }
        }),
    ]
}

pub(super) fn needs_evidence(name: &str) -> bool {
    matches!(name, "memory_edit" | "memory_forget")
}

pub(super) async fn edit(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let op = match args["op"].as_str() {
        Some("add") => Op::Add,
        Some("replace") => Op::Replace,
        Some("supersede") => Op::Supersede,
        Some("note") => Op::Note,
        Some("forget") => return Err("op must be add, replace, supersede or note.".into()),
        _ => return Err("op must be add, replace, supersede or note.".into()),
    };
    write(d, args, op).await
}

pub(super) async fn forget(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    write(d, args, Op::Forget).await
}

pub(super) async fn search(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (slug, parent) = scope(d, args).await?;
    let query = args["query"].as_str().unwrap_or("").trim();
    if query.is_empty() {
        return Err("Pass a query.".into());
    }
    let asked = args["scope"].as_str().unwrap_or("all");
    let include_shared = asked != "bot";
    let mut hits = stamp(
        &slug,
        &search_memory(d.memory, &slug, query, 20, include_shared)
            .await
            .map_err(|err| err.to_string())?,
    );
    if let Some(parent) = parent.filter(|_| asked != "shared") {
        hits.extend(stamp(
            &parent,
            &search_memory(d.memory, &parent, query, 20, false)
                .await
                .map_err(|err| err.to_string())?,
        ));
    }
    if asked == "bot" {
        hits.retain(|hit| hit["scope"] != "shared");
    } else if asked == "shared" {
        hits.retain(|hit| hit["scope"] == "shared");
    }
    hits.sort_by(|a, b| {
        b["score"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["score"].as_i64().unwrap_or(0))
            .then_with(|| {
                a["path"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["path"].as_str().unwrap_or(""))
            })
            .then_with(|| {
                a["line"]
                    .as_i64()
                    .unwrap_or(0)
                    .cmp(&b["line"].as_i64().unwrap_or(0))
            })
    });
    hits.truncate(20);
    let note = if hits.is_empty() {
        "Nothing found. Try fewer words or a different phrase."
    } else {
        "Read one result with memory_read topic=<path>."
    };
    Ok(json!({ "query": query, "results": hits, "note": note }))
}

async fn write(d: &Deps<'_>, args: &Value, op: Op) -> Result<Value, String> {
    let (slug, parent) = scope(d, args).await?;
    let scope = write_scope(args)?;
    if parent.is_some() && scope == Scope::Shared {
        return Err("A subagent writes only its own memory.".into());
    }
    let Some(expected) = args["expectedRevision"].as_str() else {
        return Err(
            "expectedRevision is required; read memory first (empty string only for a new note)."
                .into(),
        );
    };
    let quote = args["sourceQuote"].as_str().unwrap_or("").trim();
    if quote.is_empty() {
        return Err(
            "Pass the user's exact words in sourceQuote. Without them, propose the fact with memory_suggest."
                .into(),
        );
    }
    let messages = owner_messages(d.turn_items);
    let message = pick_message(&messages, args["sourceMessageId"].as_str())?;
    require_unreviewed(d.memory, &slug, d.turn_items, &message.id).await?;
    check_owner_evidence(&message.text, quote).map_err(|err| err.message)?;
    if message.attachments > 0 && generic_attachment_quote(quote) {
        return Err("That message only points at an attachment. Quote the user's own sentence that states the fact, or ask them to type it.".into());
    }
    let now = crate::vault::now_iso();
    let source_text = message.text.clone();
    let message_id = message.id.clone();
    let mut mutation = Mutation::new(&slug, op, expected);
    mutation.scope = scope;
    mutation.author = &slug;
    mutation.source_message_id = &message_id;
    mutation.source_quote = quote;
    mutation.source_text = &source_text;
    let label = bot_label(d, args, &slug);
    mutation.now = &now;
    mutation.user_root = d.bots_root;
    mutation.bot_name = &label;
    mutation.restore = args["restore"] == true;
    mutation.fact = args["fact"].as_str();
    mutation.old = args["old"].as_str();
    mutation.new_fact = args["new"].as_str();
    mutation.category = args["category"].as_str();
    mutation.path = args["path"].as_str();
    mutation.description = args["description"].as_str();
    mutation.content = args["content"].as_str();
    mutation.valid_from = args["validFrom"].as_str();
    match mutate(d.memory, mutation).await {
        Ok(result) => {
            if let Some(refreshed) = d.refreshed {
                refreshed
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(slug);
            }
            Ok(ok_json(&result))
        }
        Err(MutateFail::Code(coded)) => Err(fail_json(&coded)),
        Err(MutateFail::Io(err)) => Err(err.to_string()),
    }
}

fn bot_label(d: &Deps<'_>, args: &Value, slug: &str) -> String {
    caller(d.roster, args)
        .ok()
        .and_then(|bot| bot["name"].as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| slug.to_string())
}

fn write_scope(args: &Value) -> Result<Scope, String> {
    match args["scope"].as_str() {
        None | Some("") | Some("bot") => Ok(Scope::Bot),
        Some("shared") => Ok(Scope::Shared),
        Some(_) => Err("scope must be bot or shared.".into()),
    }
}

fn stamp(slug: &str, hits: &[Hit]) -> Vec<Value> {
    hits.iter()
        .map(|hit| {
            json!({
                "scope": hit.scope,
                "slug": slug,
                "path": hit.path,
                "line": hit.line,
                "text": hit.text,
                "score": hit.score,
                "source": hit.source,
            })
        })
        .collect()
}

fn ok_json(result: &MutateOk) -> Value {
    let mut value = json!({
        "ok": true,
        "scope": result.scope.as_str(),
        "path": result.path,
        "revision": result.revision,
    });
    if let Some(line) = &result.line {
        value["line"] = json!(line);
    }
    if let Some(forgotten) = &result.forgotten {
        value["forgotten"] = json!(forgotten);
    }
    if let Some(note) = &result.note {
        value["note"] = json!(note);
    }
    value
}

fn fail_json(coded: &Coded) -> String {
    let mut value = json!({
        "ok": false,
        "code": coded.code,
        "message": coded.message,
    });
    if let Some(revision) = &coded.revision {
        value["revision"] = json!(revision);
    }
    if let Some(content) = &coded.content {
        value["content"] = json!(content);
    }
    value.to_string()
}
