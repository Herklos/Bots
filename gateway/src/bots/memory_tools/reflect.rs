//! `memory_reflect` and `memory_reflect_apply`.
//! The messages come from the thread projection's turn items, not the chat transcript.
use super::{caller, scope, Deps};
use crate::bots::memory::{reflect_apply, reflect_packet, ApplyInput, ApplyOp, Op, ReflectError};
use serde_json::{json, Value};

#[cfg(test)]
#[path = "reflect_tests.rs"]
mod tests;

pub(super) fn defs(thread: &Value) -> Vec<Value> {
    let quote = json!({
        "type": "string",
        "description": "The user's exact typed phrase that supports the change."
    });
    vec![
        json!({
            "name": "memory_reflect",
            "description": "Read the owner's own messages after the reflection bookmark. Routine runs, group messages, and Bot messages are left out. Each message has a position in that owner stream. Use memory_reflect_apply to save facts from them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "How many messages to return after the bookmark. The default is 40 and the maximum is 80." },
                    "threadId": thread
                },
                "required": ["threadId"]
            },
            "annotations": { "readOnlyHint": true }
        }),
        json!({
            "name": "memory_reflect_apply",
            "description": "Save up to 10 add, replace, or supersede changes from a memory_reflect packet. Quote the user's exact words. throughPosition and lastMessageId name the last owner message you considered; the bookmark moves only when the whole batch succeeds. An empty operations list still marks those messages reviewed. This does not forget facts or write notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "expectedRevision": { "type": "string", "description": "The revision from memory_reflect or memory_read. Required when operations is not empty." },
                    "throughPosition": { "type": "integer", "description": "The owner-stream position of the last message you considered." },
                    "lastMessageId": { "type": "string", "description": "The id of the message at throughPosition." },
                    "operations": {
                        "type": "array",
                        "maxItems": 10,
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": { "type": "string", "enum": ["add", "replace", "supersede"] },
                                "fact": { "type": "string", "description": "The fact for add." },
                                "old": { "type": "string", "description": "The exact existing line for replace or supersede." },
                                "new": { "type": "string", "description": "The new fact for replace or supersede." },
                                "category": { "type": "string" },
                                "validFrom": { "type": "string", "description": "YYYY-MM-DD. Required for supersede." },
                                "sourceMessageId": { "type": "string" },
                                "sourceQuote": quote
                            },
                            "required": ["op", "sourceMessageId", "sourceQuote"]
                        }
                    },
                    "threadId": thread
                },
                "required": ["throughPosition", "lastMessageId", "operations", "threadId"]
            },
            "annotations": { "destructiveHint": true }
        }),
    ]
}

pub(super) fn needs_projection(name: &str) -> bool {
    matches!(name, "memory_reflect" | "memory_reflect_apply")
}

pub(super) async fn reflect(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (slug, _) = scope(d, args).await?;
    let limit = match args.get("limit") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .ok_or("limit must be a whole number from 1 to 80.")?,
        ),
    };
    reflect_packet(d.memory, &slug, d.turn_items, limit)
        .await
        .map_err(fail_json)
}

pub(super) async fn apply(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (slug, _) = scope(d, args).await?;
    let through = args["throughPosition"]
        .as_i64()
        .ok_or("throughPosition must be a non-negative integer.")?;
    let last_id = args["lastMessageId"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or("lastMessageId is required.")?;
    let operations = args["operations"]
        .as_array()
        .ok_or("operations must be a list of at most 10 changes.")?;
    let ops = operations
        .iter()
        .map(parse_op)
        .collect::<Result<Vec<_>, _>>()?;
    let expected = args["expectedRevision"].as_str().unwrap_or("");
    if !ops.is_empty() && args["expectedRevision"].as_str().is_none() {
        return Err(
            "expectedRevision is required when operations is not empty. Read memory_reflect again."
                .into(),
        );
    }
    let now = crate::vault::now_iso();
    let label = bot_label(d, args, &slug);
    let input = ApplyInput {
        slug: &slug,
        author: &slug,
        bot_name: &label,
        expected_revision: expected,
        through_position: through,
        last_message_id: last_id,
        now: &now,
        user_root: d.bots_root,
        operations: &ops,
        items: d.turn_items,
    };
    match reflect_apply(d.memory, input).await {
        Ok(value) => {
            mark_refreshed(d, &slug);
            Ok(value)
        }
        Err(err) => {
            if err.committed {
                mark_refreshed(d, &slug);
            }
            Err(fail_json(err))
        }
    }
}

fn parse_op(value: &Value) -> Result<ApplyOp, String> {
    let op = match value["op"].as_str() {
        Some("add") => Op::Add,
        Some("replace") => Op::Replace,
        Some("supersede") => Op::Supersede,
        _ => return Err("op must be add, replace, or supersede.".into()),
    };
    let source_message_id = value["sourceMessageId"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or("Each change needs a sourceMessageId from memory_reflect.")?
        .to_string();
    let source_quote = value["sourceQuote"]
        .as_str()
        .map(str::trim)
        .filter(|quote| !quote.is_empty())
        .ok_or("Pass the user's exact words in sourceQuote.")?
        .to_string();
    Ok(ApplyOp {
        op,
        fact: value["fact"].as_str().map(str::to_string),
        old: value["old"].as_str().map(str::to_string),
        new_fact: value["new"].as_str().map(str::to_string),
        category: value["category"].as_str().map(str::to_string),
        valid_from: value["validFrom"].as_str().map(str::to_string),
        source_message_id,
        source_quote,
        expected_revision: None,
    })
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

fn mark_refreshed(d: &Deps<'_>, slug: &str) {
    if let Some(refreshed) = d.refreshed {
        refreshed
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push(slug.to_string());
    }
}

fn fail_json(err: ReflectError) -> String {
    let mut value = json!({
        "ok": false,
        "code": err.code,
        "message": err.message,
        "committed": err.committed,
    });
    if let Some(revision) = err.revision {
        value["revision"] = json!(revision);
    }
    if let Some(content) = err.content {
        value["content"] = json!(content);
    }
    value.to_string()
}
