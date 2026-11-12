use super::*;

/// The last segment of a tool name: `mcp__t3-code__schedule_task` is `schedule_task`.
pub(crate) fn tool_of(item: &Value) -> &str {
    let raw = item["toolName"].as_str().unwrap_or("");
    raw.rsplit("__").next().unwrap_or(raw)
}

/// T3 sends some fields as JSON text; anything unreadable reads as an empty object.
pub(crate) fn parsed(value: &Value) -> Value {
    match value {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
        Value::Null => json!({}),
        other => other.clone(),
    }
}

/// The files of a message, as a card needs them.
pub(crate) fn files_of(attachments: &[Value]) -> Vec<Value> {
    attachments
        .iter()
        .map(|a| {
            let mime = a["mimeType"].as_str().unwrap_or("");
            let kind = if a["type"] == "image" || mime.starts_with("image/") {
                "image"
            } else if a["type"] == "audio" || mime.starts_with("audio/") {
                "audio"
            } else {
                "file"
            };
            json!({
                "id": a["id"], "name": a["name"].as_str().or(a["fileName"].as_str()).unwrap_or("Attachment"),
                "mime": mime, "bytes": a["sizeBytes"].as_u64().unwrap_or(0), "kind": kind,
            })
        })
        .collect()
}

pub(crate) fn peer(peers: &Peers, thread: &Value) -> Value {
    thread
        .as_str()
        .and_then(|t| peers.get(t))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A provider `type=error` item, reduced to the fields a transcript event can carry.
/// `None` when this is not an error item or `failure` is missing. A missing or unknown
/// `failure.class` is `other`. A completed retry is not terminal.
pub(crate) fn provider_failure(item: &Value) -> Option<Value> {
    if item["type"].as_str() != Some("error") {
        return None;
    }
    let failure = item.get("failure").filter(|value| value.is_object())?;
    let kind = match failure["class"].as_str() {
        Some("usage_limit") => "rate_limit",
        Some("transport_error") => "transport",
        _ => "other",
    };
    let retry = item.get("retry").filter(|value| value.is_object());
    let number = |key: &str| {
        retry
            .and_then(|retry| retry.get(key))
            .and_then(Value::as_u64)
    };
    Some(json!({
        "kind": kind,
        "terminal": item["status"].as_str() == Some("failed"),
        "attempt": number("attempt"),
        "maxAttempts": number("maxAttempts"),
        "retryDelayMs": number("retryDelayMs"),
        "resetAt": failure["resetAt"].as_str(),
        "message": failure["message"].as_str().unwrap_or(""),
    }))
}

/// A line the transcript shows between messages: who the Bot messaged, who messaged it, a
/// routine made or run, a one-off child thread started, or a provider failure that stopped.
/// A retry that finished (`status` completed) stays quiet. Anything else in a turn is not for the person.
pub(crate) fn event_for(item: &Value, peers: &Peers, ordinal: i64) -> Option<Value> {
    let id = format!("event-{}", item["id"].as_str().unwrap_or(""));
    let at = item["startedAt"]
        .as_str()
        .or(item["updatedAt"].as_str())
        .unwrap_or("");
    let line = |kind: &str, text: &str, title: &str, bot: Value| {
        Some(
            json!({ "id": id, "ordinal": ordinal, "at": at, "kind": kind, "text": text, "title": title, "bot": bot }),
        )
    };
    match item["type"].as_str()? {
        "dynamic_tool" if item["status"] == "completed" => {
            let input = parsed(&item["input"]);
            match tool_of(item) {
                "t3_thread_send" => line("sent", "Messaged", "", peer(peers, &input["threadId"])),
                "schedule_task" if input["scheduledTaskId"].is_null() => line(
                    "routine-created",
                    "Created routine",
                    input["title"].as_str().unwrap_or(""),
                    Value::Null,
                ),
                "schedule_task" | "update_scheduled_task" => line(
                    "routine-updated",
                    "Updated routine",
                    input["title"].as_str().unwrap_or(""),
                    Value::Null,
                ),
                "delete_scheduled_task" => {
                    line("routine-deleted", "Deleted routine", "", Value::Null)
                }
                _ => None,
            }
        }
        "user_message" if item["senderThreadId"].is_string() => line(
            "from",
            "Message from",
            "",
            peer(peers, &item["senderThreadId"]),
        ),
        "user_message" if item["scheduledTaskId"].is_string() => {
            line("ran", "Routine ran", "", Value::Null)
        }
        // A T3 child thread for this turn, not a lasting hbots Subagent.
        "subagent" => line(
            "started",
            "Started child thread",
            item["title"].as_str().unwrap_or("one-off work"),
            Value::Null,
        ),
        // Only a terminal failure is a line. The same short verbs as the other events; the
        // provider's own message is the title.
        "error" => {
            let classified = provider_failure(item)?;
            if classified["terminal"] != true {
                return None;
            }
            let (kind, text) = if classified["kind"] == "rate_limit" {
                ("provider-limit", "Limited")
            } else {
                ("provider-error", "Failed")
            };
            line(
                kind,
                text,
                classified["message"].as_str().unwrap_or(""),
                Value::Null,
            )
            .map(|mut event| {
                event["failure"] = classified;
                event
            })
        }
        _ => None,
    }
}

/// What a pending request needs to be answered from the app: its id for the respond route
/// (not the item's own id) and the choices it offers.
pub(crate) fn request_for(item: &Value) -> Value {
    let kind = item["type"].as_str().unwrap_or("");
    let item_id = item["id"].clone();
    let id = item["requestId"]
        .as_str()
        .map(Value::from)
        .unwrap_or_else(|| item_id.clone());
    match kind {
        "approval_request" => json!({
            "id": id, "itemId": item_id, "kind": kind, "runId": item["runId"],
            "title": item["prompt"].as_str().or(item["title"].as_str()).unwrap_or("The Bot is asking"),
            "reason": item["reason"].as_str().unwrap_or(""),
            "requestKind": item["requestKind"].as_str().unwrap_or(""),
            "options": item["options"].as_array().cloned().unwrap_or_default(),
        }),
        "user_input_request" => {
            let questions = parsed(&item["questions"]);
            let questions = questions.as_array().cloned().unwrap_or_default();
            let first = questions
                .first()
                .and_then(|q| q["question"].as_str())
                .or(item["title"].as_str())
                .unwrap_or("The Bot is asking");
            json!({
                "id": id, "itemId": item_id, "kind": kind, "runId": item["runId"],
                "title": first, "questions": questions,
            })
        }
        _ => json!({
            "id": item_id, "itemId": item_id, "kind": kind, "runId": item["runId"],
            "title": item["label"].as_str().or(item["title"].as_str()).unwrap_or("The Bot is asking"),
            "reason": item["reason"].as_str().unwrap_or(""),
            "placeholder": item["placeholder"].as_str().unwrap_or(""),
        }),
    }
}

/// Drops framed turns from a one-to-one chat: a group dispatch (`[Group "…`) or a Computer
/// open nudge (`[Computer]`), plus everything the Bot did in answer, up to the next ordinary
/// message from the person. Group rooms use [`group_transcript`] and keep their own frames.
pub(crate) fn without_group_turns(items: Vec<Value>) -> Vec<Value> {
    let mut hidden = false;
    items
        .into_iter()
        .filter(|raw| {
            let item = if raw["item"].is_object() {
                &raw["item"]
            } else {
                raw
            };
            if item["type"] == "user_message" {
                hidden = item["text"].as_str().is_some_and(is_framed_prompt);
            }
            !hidden
        })
        .collect()
}

pub(crate) fn turn_items(projection: &Value) -> Vec<Value> {
    projection["turnItems"]
        .as_array()
        .or_else(|| projection["visibleTurnItems"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// The bubbles and cards of a thread projection, oldest first.
pub(crate) fn transcript(projection: &Value) -> Value {
    transcript_for(projection, &Peers::new())
}

/// Like [`transcript`], with the lines between messages (`events`) and each message's files.
/// `ordinal` orders messages and events together.
pub(crate) fn transcript_for(projection: &Value, peers: &Peers) -> Value {
    project_turns(
        &without_group_turns(turn_items(projection)),
        peers,
        projection,
    )
}

/// The same shape as [`transcript`], for a thread that is itself the group.
/// A group thread has no one-to-one sender line, so this takes no peers.
/// The one-to-one filter stays on [`transcript`]: it would drop the framed user message and the reply.
pub(crate) fn group_transcript(projection: &Value) -> Value {
    project_turns(&turn_items(projection), &Peers::new(), projection)
}

pub(crate) fn project_turns(items: &[Value], peers: &Peers, projection: &Value) -> Value {
    let mut messages = Vec::new();
    let mut events = Vec::new();
    let mut requests = Vec::new();
    let mut working = false;
    for (index, raw) in items.iter().enumerate() {
        let item = if raw["item"].is_object() {
            raw["item"].clone()
        } else {
            raw.clone()
        };
        let kind = item["type"].as_str().unwrap_or("");
        let status = item["status"].as_str().unwrap_or("");
        let live = LIVE.contains(&status) || item["streaming"] == true;
        let ordinal = item["ordinal"]
            .as_i64()
            .unwrap_or_else(|| i64::try_from(index).unwrap_or(i64::MAX));
        if let Some(event) = event_for(&item, peers, ordinal) {
            events.push(event);
        }
        match kind {
            "user_message" | "assistant_message" => {
                let text = item["text"].as_str().unwrap_or("").trim().to_string();
                let attachments = item["attachments"].as_array().cloned().unwrap_or_default();
                if text.is_empty() && attachments.is_empty() && !live {
                    continue;
                }
                let sender = peer(peers, &item["senderThreadId"]);
                messages.push(json!({
                    "id": item["id"], "role": if kind == "user_message" { "user" } else { "bot" }, "text": text,
                    "files": files_of(&attachments), "attachments": attachments, "ordinal": ordinal,
                    "from": sender,
                    "at": item["startedAt"].as_str().or(item["updatedAt"].as_str()).unwrap_or(""), "streaming": live,
                }));
            }
            "approval_request" | "user_input_request" | "secret_request"
                if live || status == "awaiting_response" || status == "waiting" =>
            {
                requests.push(request_for(&item));
            }
            _ => {}
        }
        working |= live && kind != "user_message";
    }
    let runs = projection["runs"].as_array().cloned().unwrap_or_default();
    let active = runs
        .iter()
        .rev()
        .find(|r| r["status"].as_str().is_some_and(|s| LIVE.contains(&s)))
        .and_then(|r| r["id"].as_str().map(String::from));
    json!({ "messages": messages, "events": events, "requests": requests, "working": working || active.is_some(), "activeRunId": active })
}

pub(crate) fn projection_partial(projection: &Value) -> bool {
    projection["truncated"] == true
        || projection["hasMore"] == true
        || projection["partial"] == true
}

/// Adds a token line only where [`crate::bots::usage::turn_token_line`] finds a shared id.
pub(crate) fn with_tokens(mut transcript: Value, projection: &Value) -> Value {
    if let Some(messages) = transcript.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages.iter_mut() {
            let id = message["id"].as_str().unwrap_or("");
            message["tokens"] =
                crate::bots::usage::turn_token_line(projection, id).unwrap_or(Value::Null);
        }
    }
    transcript["partial"] = json!(projection_partial(projection));
    transcript
}

/// The command a phone send dispatches.
/// `attachments` is the gateway's file records (`crate::chat::attachments::command_part`).
pub(crate) fn send_command(thread: &str, text: &str, attachments: &[Value]) -> Value {
    command_with(thread, text, attachments, &uuid::Uuid::new_v4().to_string())
}

pub(crate) fn command_with(thread: &str, text: &str, attachments: &[Value], id: &str) -> Value {
    json!({
        "commandId": id, "type": "message.dispatch", "threadId": thread, "messageId": id, "text": text,
        "attachments": attachments, "createdBy": "user", "creationSource": "web",
        "deliveryIntent": "steer", "dispatchMode": { "type": "start_immediately" },
    })
}

/// A short token the client chose, or a new id. The transcript row is `turn-item:message:` plus this.
pub(crate) fn message_id(supplied: Option<&str>) -> String {
    let id = supplied.unwrap_or("").trim();
    if (1..=80).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        id.to_string()
    } else {
        uuid::Uuid::new_v4().to_string()
    }
}

/// A send aimed at a child. The command is built only after the child resolves, so a remote
/// child or the parent id produces no command and does not write the parent. No quote field.
#[cfg(test)]
pub(crate) fn child_send(
    rows: &[Value],
    parent: &str,
    asked: &str,
    text: &str,
    attachments: &[Value],
) -> Result<Value, &'static str> {
    child_send_with(
        rows,
        parent,
        asked,
        text,
        attachments,
        &uuid::Uuid::new_v4().to_string(),
    )
}

pub(crate) fn child_send_with(
    rows: &[Value],
    parent: &str,
    asked: &str,
    text: &str,
    attachments: &[Value],
    id: &str,
) -> Result<Value, &'static str> {
    let thread = crate::bots::lineage::thread_for(rows, parent, Some(asked))?;
    Ok(command_with(&thread, text, attachments, id))
}
