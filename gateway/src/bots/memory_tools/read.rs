//! Extended `memory_read`: revisions, notes, shared memory, and one note.
use super::{memory_of, scope, Deps};
use crate::bots::memory::{
    document, list_notes, read_note, NoteInfo, FACT_CHARS, FULL_NOTE_CHARS, MEMORY_CHARS,
    NOTE_CHARS,
};
use serde_json::{json, Value};

pub(super) async fn read(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (own, parent) = scope(d, args).await?;
    let topic = args["topic"]
        .as_str()
        .map(str::trim)
        .filter(|topic| !topic.is_empty());
    if let Some(topic) = topic {
        let max = if args["full"] == true {
            FULL_NOTE_CHARS
        } else {
            NOTE_CHARS
        };
        let note = read_note(d.memory, &own, topic, max).await?;
        let mut result = json!({
            "scope": "bot",
            "topic": note.path,
            "content": note.text,
            "revision": note.revision,
        });
        if args["full"] == true {
            result["full"] = json!(true);
        }
        return Ok(result);
    }
    let asked = match args["scope"].as_str() {
        Some("bot") => "bot",
        Some("shared") => "shared",
        _ => "all",
    };
    let mut result = json!({ "memory": memory_of(d.memory, &own).await? });
    if let Some(parent) = &parent {
        result["parent"] = json!({ "memory": memory_of(d.memory, parent).await? });
    }
    if asked != "shared" {
        let bot = document(
            d.memory,
            &crate::bots::memory::MemoryStore::bot_memory(&own),
            MEMORY_CHARS,
        )
        .await
        .map_err(|err| err.to_string())?;
        result["revision"] = json!(bot.revision);
        if bot.truncated {
            result["botTruncated"] = json!(true);
        }
        result["bot"] = json!(bot.text);
        result["notes"] = json!(notes_json(
            &list_notes(d.memory, &own)
                .await
                .map_err(|err| err.to_string())?,
        ));
    }
    if asked != "bot" {
        let shared = document(
            d.memory,
            crate::bots::memory::MemoryStore::shared_memory(),
            MEMORY_CHARS,
        )
        .await
        .map_err(|err| err.to_string())?;
        result["shared"] = json!(shared.text);
        result["sharedRevision"] = json!(shared.revision);
        if shared.truncated {
            result["sharedTruncated"] = json!(true);
        }
    }
    result["limits"] = json!({
        "memoryChars": MEMORY_CHARS,
        "factChars": FACT_CHARS,
        "noteChars": NOTE_CHARS,
    });
    Ok(result)
}

fn notes_json(notes: &[NoteInfo]) -> Vec<Value> {
    notes
        .iter()
        .map(|note| {
            json!({
                "path": note.path,
                "description": note.description,
                "chars": note.chars,
                "tokens": note.tokens,
                "updatedAt": note.updated_at,
            })
        })
        .collect()
}
