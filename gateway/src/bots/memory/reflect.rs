//! Reflection: read the owner stream past the bookmark, then apply a short batch.
//! The bookmark moves only after every operation in the batch has succeeded.

use super::evidence::check_owner_evidence;
use super::facts::{hash_fact, local_date, parse_fact};
use super::mutate::mutate_locked;
use super::owner_stream::{owner_stream, OwnerTurn};
use super::state::{self, Reflection};
use super::{MemoryStore, MutateFail, Mutation, Op};
use serde_json::{json, Value};
use std::path::Path;

const DEFAULT_LIMIT: usize = 40;
const MAX_LIMIT: usize = 80;
const MAX_OPS: usize = 10;

const INSTRUCTIONS: &str = "\
These are the owner's own messages after the reflection bookmark, oldest first. \
Position is the index in that owner stream. Routine runs, group messages, and Bot messages are left out. \
Save lasting facts with memory_reflect_apply: at most 10 add, replace, or supersede operations. \
Quote the user's exact words in sourceQuote. \
throughPosition is the position of the last message you considered, and lastMessageId is that message's id. \
Replace and supersede together can change at most one quarter of the existing fact lines, rounded down. \
An empty operations list still marks the stream reviewed through that message.";

#[derive(Debug)]
pub(crate) struct ReflectError {
    pub code: &'static str,
    pub message: String,
    pub revision: Option<String>,
    pub content: Option<String>,
    pub committed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ApplyOp {
    pub op: Op,
    pub fact: Option<String>,
    pub old: Option<String>,
    pub new_fact: Option<String>,
    pub category: Option<String>,
    pub valid_from: Option<String>,
    pub source_message_id: String,
    pub source_quote: String,
    /// Set only to force one op onto a stale revision after an earlier op committed.
    pub expected_revision: Option<String>,
}

pub(crate) struct ApplyInput<'a> {
    pub slug: &'a str,
    pub author: &'a str,
    pub bot_name: &'a str,
    pub expected_revision: &'a str,
    pub through_position: i64,
    pub last_message_id: &'a str,
    pub now: &'a str,
    pub user_root: Option<&'a Path>,
    pub operations: &'a [ApplyOp],
    pub items: &'a [Value],
}

pub(crate) async fn packet(
    store: &MemoryStore,
    slug: &str,
    items: &[Value],
    limit: Option<i64>,
) -> Result<Value, ReflectError> {
    let limit = page_limit(limit)?;
    let _guard = store.lock_file(&format!("bot:{slug}")).await;
    let state = state::read_state(store, &MemoryStore::bot_state(slug))
        .await
        .map_err(io_error)?;
    let bookmark = state.reflection.last_position.unwrap_or(-1);
    let messages: Vec<Value> = owner_stream(items)
        .into_iter()
        .filter(|turn| turn.position > bookmark)
        .take(limit)
        .map(message_json)
        .collect();
    let revision = memory_revision(store, slug).await?;
    Ok(json!({
        "messages": messages,
        "revision": revision,
        "bookmark": bookmark_json(&state.reflection),
        "instructions": INSTRUCTIONS,
    }))
}

pub(crate) async fn apply(
    store: &MemoryStore,
    input: ApplyInput<'_>,
) -> Result<Value, ReflectError> {
    let slug = input.slug;
    let _guard = store.lock_file(&format!("bot:{slug}")).await;
    let file = MemoryStore::bot_memory(slug);
    let state_file = MemoryStore::bot_state(slug);
    let stream = owner_stream(input.items);
    let mut memory_state = state::read_state(store, &state_file)
        .await
        .map_err(io_error)?;
    check_mark(
        &stream,
        input.through_position,
        input.last_message_id,
        memory_state.reflection.last_position,
    )?;
    if input.operations.len() > MAX_OPS {
        return Err(invalid(
            "cap",
            format!("A reflection applies at most {MAX_OPS} changes."),
        ));
    }
    for op in input.operations {
        reflect_op(op.op)?;
    }
    let mut content = store.read_text_or_empty(&file).await.map_err(io_error)?;
    let mut running = state::revision_of(&content);
    if !input.operations.is_empty() && running != input.expected_revision {
        return Err(conflict(&running, &content));
    }
    let replaces = input
        .operations
        .iter()
        .filter(|op| matches!(op.op, Op::Replace | Op::Supersede))
        .count();
    let room = fact_lines(&content) / 4;
    if replaces > room {
        return Err(invalid(
            "cap",
            format!("A reflection can replace or supersede at most {room} fact lines right now."),
        ));
    }
    check_sources(
        &stream,
        input.operations,
        memory_state.reflection.last_position,
        input.through_position,
    )?;
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    for op in input.operations {
        if already_applied(&content, op) {
            skipped.push(json!({
                "op": op.op.as_str(),
                "reason": "already in memory",
            }));
            continue;
        }
        let expected = op.expected_revision.as_deref().unwrap_or(running.as_str());
        let quote = op.source_quote.clone();
        let message_id = op.source_message_id.clone();
        let source_text = stream
            .iter()
            .find(|turn| turn.id == message_id)
            .map(|turn| turn.text.clone())
            .unwrap_or_default();
        let fact = op.fact.clone().unwrap_or_default();
        let old = op.old.clone().unwrap_or_default();
        let new_fact = op.new_fact.clone().unwrap_or_default();
        let category = op.category.clone().unwrap_or_default();
        let valid_from = op.valid_from.clone().unwrap_or_default();
        let mut mutation = Mutation::new(input.slug, op.op, expected);
        mutation.author = input.author;
        mutation.source_message_id = &message_id;
        mutation.source_quote = &quote;
        mutation.source_text = &source_text;
        mutation.now = input.now;
        mutation.user_root = input.user_root;
        mutation.bot_name = input.bot_name;
        mutation.fact = Some(fact.as_str());
        mutation.old = Some(old.as_str());
        mutation.new_fact = Some(new_fact.as_str());
        if !category.is_empty() {
            mutation.category = Some(category.as_str());
        }
        if !valid_from.is_empty() {
            mutation.valid_from = Some(valid_from.as_str());
        }
        match mutate_locked(store, mutation).await {
            Ok(result) => {
                applied.push(json!({
                    "op": op.op.as_str(),
                    "line": result.line,
                }));
                running = result.revision;
                content = store
                    .read_text_or_empty(&file)
                    .await
                    .map_err(|err| io_after(err, &running))?;
            }
            Err(MutateFail::Code(coded)) if coded.code == "duplicate" => {
                skipped.push(json!({
                    "op": op.op.as_str(),
                    "reason": coded.message,
                }));
            }
            Err(MutateFail::Code(coded)) => {
                return Err(ReflectError {
                    code: coded.code,
                    message: coded.message,
                    revision: coded.revision.or(Some(running)),
                    content: coded.content,
                    committed: !applied.is_empty(),
                });
            }
            Err(MutateFail::Io(err)) => return Err(io_after(err, &running)),
        }
    }
    let line = dreams_line(
        input.now,
        input.through_position,
        input.last_message_id,
        applied.len(),
        skipped.len(),
    );
    append_dreams(store, input.slug, &line)
        .await
        .map_err(|err| io_after(err, &running))?;
    memory_state = state::read_state(store, &state_file)
        .await
        .map_err(|err| io_after(err, &running))?;
    memory_state.reflection = Reflection {
        last_position: Some(input.through_position),
        last_message_id: Some(input.last_message_id.to_string()),
        at: Some(input.now.to_string()),
    };
    state::write_state(store, &state_file, &memory_state)
        .await
        .map_err(|err| io_after(err, &running))?;
    Ok(json!({
        "ok": true,
        "applied": applied,
        "skipped": skipped,
        "revision": running,
        "bookmark": bookmark_json(&memory_state.reflection),
    }))
}

/// A reviewed message cannot stand in for a new reflection.
pub(crate) async fn require_unreviewed(
    store: &MemoryStore,
    slug: &str,
    items: &[Value],
    message_id: &str,
) -> Result<(), String> {
    let state = state::read_state(store, &MemoryStore::bot_state(slug))
        .await
        .map_err(|err| err.to_string())?;
    let Some(bookmark) = state.reflection.last_position else {
        return Ok(());
    };
    let reviewed = owner_stream(items)
        .into_iter()
        .any(|turn| turn.id == message_id && turn.position <= bookmark);
    if reviewed {
        return Err(
            "That message is already reflected. Call memory_reflect for messages after the bookmark."
                .into(),
        );
    }
    Ok(())
}

fn page_limit(limit: Option<i64>) -> Result<usize, ReflectError> {
    let Some(limit) = limit else {
        return Ok(DEFAULT_LIMIT);
    };
    if limit < 1 {
        return Err(invalid(
            "limit",
            "limit must be a whole number from 1 to 80.",
        ));
    }
    Ok(usize::try_from(limit).unwrap_or(MAX_LIMIT).min(MAX_LIMIT))
}

fn check_mark(
    stream: &[OwnerTurn],
    through: i64,
    last_id: &str,
    bookmark: Option<i64>,
) -> Result<(), ReflectError> {
    if through < 0 {
        return Err(invalid(
            "negative",
            "throughPosition must be a non-negative integer.",
        ));
    }
    let Some(end) = stream.last() else {
        return Err(invalid(
            "skip_ahead",
            "throughPosition skips ahead of the owner stream.",
        ));
    };
    if through > end.position {
        return Err(invalid(
            "skip_ahead",
            "throughPosition skips ahead of the owner stream.",
        ));
    }
    let at = stream.iter().find(|turn| turn.position == through);
    match at {
        Some(turn) if turn.id == last_id => {}
        Some(_) | None => {
            return Err(invalid(
                "mismatch",
                "lastMessageId does not match throughPosition in the owner stream.",
            ));
        }
    }
    if through <= bookmark.unwrap_or(-1) {
        return Err(invalid(
            "bookmark",
            "throughPosition must be past the reflection bookmark.",
        ));
    }
    Ok(())
}

fn check_sources(
    stream: &[OwnerTurn],
    operations: &[ApplyOp],
    bookmark: Option<i64>,
    through: i64,
) -> Result<(), ReflectError> {
    let floor = bookmark.unwrap_or(-1);
    for op in operations {
        let Some(turn) = stream.iter().find(|turn| turn.id == op.source_message_id) else {
            return Err(invalid(
                "window",
                "sourceMessageId is not in the owner stream. Routine and group messages are not evidence.",
            ));
        };
        if turn.position <= floor || turn.position > through {
            return Err(invalid(
                "window",
                "sourceMessageId is outside this reflection window.",
            ));
        }
        check_owner_evidence(&turn.text, &op.source_quote).map_err(|err| ReflectError {
            code: err.code,
            message: err.message,
            revision: None,
            content: None,
            committed: false,
        })?;
    }
    Ok(())
}

fn reflect_op(op: Op) -> Result<Op, ReflectError> {
    match op {
        Op::Add | Op::Replace | Op::Supersede => Ok(op),
        Op::Note | Op::Forget => Err(invalid(
            "invalid",
            "memory_reflect_apply only adds, replaces, or supersedes. It does not forget or write notes.",
        )),
    }
}

fn already_applied(content: &str, op: &ApplyOp) -> bool {
    let new = match op.op {
        Op::Replace | Op::Supersede => op.new_fact.as_deref().unwrap_or("").trim(),
        Op::Add | Op::Note | Op::Forget => return false,
    };
    if new.is_empty() {
        return false;
    }
    let old = op.old.as_deref().unwrap_or("").trim();
    if content.lines().any(|line| line.trim() == old) {
        return false;
    }
    let key = hash_fact(new);
    content
        .lines()
        .any(|line| !line.trim().is_empty() && hash_fact(&parse_fact(line).text) == key)
}

fn fact_lines(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            (trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ "))
                && !trimmed[2..].trim().is_empty()
        })
        .count()
}

fn dreams_line(now: &str, through: i64, id: &str, applied: usize, skipped: usize) -> String {
    let date = local_date(now);
    if applied == 0 && skipped == 0 {
        format!("- {date} reviewed through {through} ({id}): nothing to save.\n")
    } else {
        format!(
            "- {date} reviewed through {through} ({id}): {applied} applied, {skipped} skipped.\n"
        )
    }
}

async fn append_dreams(store: &MemoryStore, slug: &str, line: &str) -> std::io::Result<()> {
    let rel = MemoryStore::bot_dreams(slug);
    let mut text = store.read_text_or_empty(&rel).await?;
    if text.trim().is_empty() {
        text = "# Dreams\n\n".to_string();
    } else if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    store.write_text(&rel, &text, Some(0o644)).await
}

async fn memory_revision(store: &MemoryStore, slug: &str) -> Result<String, ReflectError> {
    let text = store
        .read_text_or_empty(&MemoryStore::bot_memory(slug))
        .await
        .map_err(io_error)?;
    Ok(state::revision_of(&text))
}

fn message_json(turn: OwnerTurn) -> Value {
    json!({
        "id": turn.id,
        "text": turn.text,
        "position": turn.position,
        "at": turn.at,
    })
}

fn bookmark_json(reflection: &Reflection) -> Value {
    json!({
        "lastPosition": reflection.last_position,
        "lastMessageId": reflection.last_message_id,
        "at": reflection.at,
    })
}

fn invalid(code: &'static str, message: impl Into<String>) -> ReflectError {
    ReflectError {
        code,
        message: message.into(),
        revision: None,
        content: None,
        committed: false,
    }
}

fn conflict(revision: &str, content: &str) -> ReflectError {
    ReflectError {
        code: "conflict",
        message: "Memory changed since you read it. Read it again, merge, and retry with the new revision.".into(),
        revision: Some(revision.to_string()),
        content: Some(content.to_string()),
        committed: false,
    }
}

fn io_error(err: std::io::Error) -> ReflectError {
    ReflectError {
        code: "io",
        message: err.to_string(),
        revision: None,
        content: None,
        committed: false,
    }
}

fn io_after(err: std::io::Error, revision: &str) -> ReflectError {
    ReflectError {
        code: "io",
        message: err.to_string(),
        revision: Some(revision.to_string()),
        content: None,
        committed: true,
    }
}
