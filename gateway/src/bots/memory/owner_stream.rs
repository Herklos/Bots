//! Owner messages from a thread projection, in projection order.
//! Routine runs, group frames, and Bot or server turns are left out.
//! Positions are dense and 0-based in this filtered stream.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerTurn {
    pub id: String,
    pub text: String,
    pub position: i64,
    pub at: String,
    pub attachments: usize,
}

/// The projection's turn items, oldest first. Not the chat transcript.
pub(crate) fn owner_stream(items: &[Value]) -> Vec<OwnerTurn> {
    let mut out = Vec::new();
    for raw in items {
        let Some(turn) = owner_turn(raw) else {
            continue;
        };
        let position = i64::try_from(out.len()).unwrap_or(i64::MAX);
        out.push(OwnerTurn { position, ..turn });
    }
    out
}

fn owner_turn(raw: &Value) -> Option<OwnerTurn> {
    let item = if raw.get("item").is_some_and(Value::is_object) {
        &raw["item"]
    } else {
        raw
    };
    if item["type"] != "user_message" {
        return None;
    }
    if item
        .get("scheduledTaskId")
        .is_some_and(|value| !value.is_null())
    {
        return None;
    }
    if item["senderThreadId"]
        .as_str()
        .is_some_and(|sender| !sender.trim().is_empty())
    {
        return None;
    }
    if item["createdBy"]
        .as_str()
        .is_some_and(|created_by| created_by != "user")
    {
        return None;
    }
    if item["creationSource"]
        .as_str()
        .is_some_and(|source| source == "server" || source == "agent")
    {
        return None;
    }
    let text = item["text"].as_str().unwrap_or("").to_string();
    if text.trim().is_empty() || crate::chat::is_framed_prompt(text.trim()) {
        return None;
    }
    Some(OwnerTurn {
        id: item["id"].as_str().unwrap_or("").to_string(),
        text,
        position: 0,
        at: item["startedAt"]
            .as_str()
            .or_else(|| item["updatedAt"].as_str())
            .or_else(|| item["createdAt"].as_str())
            .unwrap_or("")
            .to_string(),
        attachments: item["attachments"].as_array().map(Vec::len).unwrap_or(0),
    })
}
