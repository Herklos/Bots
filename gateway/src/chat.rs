//! A small REST face on a Bot's main thread for clients that don't speak the harness RPC,
//! such as the Exact app's `app.ts`: read the conversation, send a message, stop a run, answer
//! an approval. T3 Code is the harness wired today; other backends may be added. The `/t3/*`
//! passthrough is that backend's door.
use serde_json::{json, Value};
use std::collections::HashMap;

const LIVE: &[&str] = &[
    "running",
    "in_progress",
    "pending",
    "queued",
    "streaming",
    "starting",
];

/// Bots by main thread: `{slug, name, color, shape}`. It turns a thread id in a tool call or a
/// sender into the Bot the person knows.
pub type Peers = HashMap<String, Value>;

/// The last segment of a tool name: `mcp__t3-code__schedule_task` is `schedule_task`.
fn tool_of(item: &Value) -> &str {
    let raw = item["toolName"].as_str().unwrap_or("");
    raw.rsplit("__").next().unwrap_or(raw)
}

/// T3 sends some fields as JSON text; anything unreadable reads as an empty object.
fn parsed(value: &Value) -> Value {
    match value {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
        Value::Null => json!({}),
        other => other.clone(),
    }
}

/// The files of a message, as a card needs them.
fn files_of(attachments: &[Value]) -> Vec<Value> {
    attachments
        .iter()
        .map(|a| {
            let mime = a["mimeType"].as_str().unwrap_or("");
            let kind = if a["type"] == "image" || mime.starts_with("image/") {
                "image"
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

fn peer(peers: &Peers, thread: &Value) -> Value {
    thread
        .as_str()
        .and_then(|t| peers.get(t))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A line the transcript shows between messages: who the Bot messaged, who messaged it, a
/// routine made or run, a helper started. Anything else in a turn is not for the person.
fn event_for(item: &Value, peers: &Peers, ordinal: i64) -> Option<Value> {
    let id = format!("event-{}", item["id"].as_str().unwrap_or(""));
    let at = item["startedAt"]
        .as_str()
        .or(item["updatedAt"].as_str())
        .unwrap_or("");
    let line = |kind: &str, text: &str, title: &str, bot: Value| {
        Some(json!({ "id": id, "ordinal": ordinal, "at": at, "kind": kind, "text": text, "title": title, "bot": bot }))
    };
    match item["type"].as_str()? {
        "dynamic_tool" if item["status"] == "completed" => {
            let input = parsed(&item["input"]);
            match tool_of(item) {
                "t3_thread_send" => line("sent", "Messaged", "", peer(peers, &input["threadId"])),
                "schedule_task" if input["scheduledTaskId"].is_null() => line(
                    "routine",
                    "Created routine",
                    input["title"].as_str().unwrap_or(""),
                    Value::Null,
                ),
                "schedule_task" | "update_scheduled_task" => line(
                    "routine",
                    "Updated routine",
                    input["title"].as_str().unwrap_or(""),
                    Value::Null,
                ),
                "delete_scheduled_task" => line("routine", "Deleted routine", "", Value::Null),
                _ => None,
            }
        }
        "user_message" if item["senderThreadId"].is_string() => {
            line("from", "Message from", "", peer(peers, &item["senderThreadId"]))
        }
        "user_message" if item["scheduledTaskId"].is_string() => {
            line("ran", "Routine ran", "", Value::Null)
        }
        "subagent" => line(
            "started",
            "Started",
            item["title"].as_str().unwrap_or("a helper"),
            Value::Null,
        ),
        _ => None,
    }
}

/// What a pending request needs to be answered from the app: its id for the respond route
/// (not the item's own id) and the choices it offers.
fn request_for(item: &Value) -> Value {
    let kind = item["type"].as_str().unwrap_or("");
    let item_id = item["id"].clone();
    let id = item["requestId"].as_str().map(Value::from).unwrap_or_else(|| item_id.clone());
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

/// The bubbles and cards of a thread projection, oldest first.
pub fn transcript(projection: &Value) -> Value {
    transcript_for(projection, &Peers::new())
}

/// Like [`transcript`], with the lines between messages (`events`) and each message's files.
/// `ordinal` orders messages and events together.
pub fn transcript_for(projection: &Value, peers: &Peers) -> Value {
    let items = projection["turnItems"]
        .as_array()
        .or_else(|| projection["visibleTurnItems"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut messages = Vec::new();
    let mut events = Vec::new();
    let mut requests = Vec::new();
    let mut working = false;
    for (index, raw) in items.into_iter().enumerate() {
        let item = if raw["item"].is_object() {
            raw["item"].clone()
        } else {
            raw
        };
        let kind = item["type"].as_str().unwrap_or("");
        let status = item["status"].as_str().unwrap_or("");
        let live = LIVE.contains(&status) || item["streaming"] == true;
        let ordinal = item["ordinal"].as_i64().unwrap_or(index as i64);
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

/// The command a phone send dispatches.
/// `attachments` is the gateway's file records (`attachments::command_part`).
pub fn send_command(thread: &str, text: &str, attachments: &[Value]) -> Value {
    let id = uuid::Uuid::new_v4().to_string();
    json!({
        "commandId": id, "type": "message.dispatch", "threadId": thread, "messageId": id, "text": text,
        "attachments": attachments, "createdBy": "user", "creationSource": "web",
        "deliveryIntent": "steer", "dispatchMode": { "type": "start_immediately" },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubbles_from_turn_items() {
        let p = json!({
            "turnItems": [
                { "id": "1", "type": "user_message", "text": "hi", "status": "completed", "startedAt": "t1" },
                { "id": "2", "type": "tool_call", "status": "completed" },
                { "id": "3", "type": "assistant_message", "text": "hello", "status": "running", "streaming": true },
                { "id": "4", "type": "approval_request", "status": "pending", "title": "Run rm?" },
            ],
            "runs": [{ "id": "r1", "status": "running" }],
        });
        let t = transcript(&p);
        assert_eq!(t["messages"].as_array().unwrap().len(), 2);
        assert_eq!(t["messages"][1]["role"], "bot");
        assert_eq!(t["requests"][0]["title"], "Run rm?");
        assert_eq!(
            (t["working"].as_bool(), t["activeRunId"].as_str()),
            (Some(true), Some("r1"))
        );
    }

    fn fixture(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/t3").join(name);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn a_real_thread_gives_messages_with_times_and_no_noise_events() {
        let p = fixture("thread-projection.json");
        let t = transcript(&p["value"]);
        assert_eq!(t["messages"].as_array().unwrap().len(), 6);
        assert!(t["messages"][0]["at"].as_str().unwrap().starts_with("2026-"));
        assert!(t["events"].as_array().unwrap().is_empty(), "reads, commands and checkpoints are not lines");
        assert_eq!(t["working"], false);
    }

    #[test]
    fn a_routine_made_by_a_bot_is_a_line_with_its_title() {
        let made = fixture("routine-created.json");
        let item = json!({ "id": "i1", "ordinal": 7, "type": "dynamic_tool", "status": "completed", "startedAt": "2026-11-10T13:05:00Z",
            "toolName": made["toolItem"]["toolName"], "input": made["toolItem"]["input"], "output": made["toolItem"]["output"] });
        let t = transcript(&json!({ "turnItems": [item] }));
        let e = &t["events"][0];
        assert_eq!((e["kind"].as_str(), e["text"].as_str(), e["title"].as_str()), (Some("routine"), Some("Created routine"), Some("hbots test")));
        assert_eq!(e["ordinal"], 7);
        // An update carries the id of the routine it changes.
        let update = json!({ "id": "i2", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3-code__schedule_task",
            "input": { "scheduledTaskId": "scheduled-task:1", "title": "Morning" } });
        assert_eq!(transcript(&json!({ "turnItems": [update] }))["events"][0]["text"], "Updated routine");
    }

    #[test]
    fn messages_between_bots_name_the_bot() {
        let mut peers = Peers::new();
        peers.insert("thread-inbox".into(), json!({ "slug": "inbox", "name": "Inbox", "color": "brand.pink", "shape": "hex" }));
        let sent = json!({ "id": "a", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3__t3_thread_send",
            "input": "{\"threadId\":\"thread-inbox\",\"message\":\"Draft ready\"}" });
        let got = json!({ "id": "b", "type": "user_message", "status": "completed", "text": "Sent you a file", "senderThreadId": "thread-inbox" });
        let t = transcript_for(&json!({ "turnItems": [sent, got] }), &peers);
        assert_eq!(t["events"][0]["text"], "Messaged");
        assert_eq!(t["events"][0]["bot"]["name"], "Inbox");
        assert_eq!(t["events"][1]["text"], "Message from");
        assert_eq!(t["messages"][0]["from"]["slug"], "inbox");
        // A message from the person has no sender.
        let mine = transcript(&json!({ "turnItems": [{ "id": "c", "type": "user_message", "status": "completed", "text": "hi" }] }));
        assert_eq!(mine["messages"][0]["from"], Value::Null);
    }

    #[test]
    fn a_retrying_error_and_a_routine_run_are_handled() {
        let made = fixture("routine-created.json");
        let retry = json!({ "id": "e", "type": "error", "status": "completed", "retry": { "attempt": 4 } });
        assert!(transcript(&json!({ "turnItems": [retry] }))["events"].as_array().unwrap().is_empty());
        let ran = json!({ "id": "r", "type": "user_message", "status": "completed", "text": "test only", "scheduledTaskId": made["taskRow"]["id"] });
        assert_eq!(transcript(&json!({ "turnItems": [ran] }))["events"][0]["text"], "Routine ran");
    }

    #[test]
    fn requests_carry_the_id_the_respond_route_needs_and_their_choices() {
        let approval = json!({ "id": "item-1", "requestId": "req-1", "type": "approval_request", "status": "pending", "prompt": "Run ls?",
            "requestKind": "command", "options": [{ "decision": "accept", "label": "Allow" }, { "decision": "decline", "label": "Deny" }] });
        let question = json!({ "id": "item-2", "requestId": "req-2", "type": "user_input_request", "status": "pending",
            "questions": [{ "id": "q1", "header": "Pick", "question": "Which one?", "options": [{ "label": "A", "description": "a" }] }] });
        let secret = json!({ "id": "item-3", "type": "secret_request", "status": "pending", "label": "API key", "reason": "To call the API", "placeholder": "sk-" });
        let t = transcript(&json!({ "turnItems": [approval, question, secret] }));
        let r = t["requests"].as_array().unwrap();
        assert_eq!((r[0]["id"].as_str(), r[0]["itemId"].as_str(), r[0]["title"].as_str()), (Some("req-1"), Some("item-1"), Some("Run ls?")));
        assert_eq!(r[0]["options"][1]["decision"], "decline");
        assert_eq!((r[1]["id"].as_str(), r[1]["title"].as_str()), (Some("req-2"), Some("Which one?")));
        assert_eq!(r[1]["questions"][0]["id"], "q1");
        assert_eq!((r[2]["id"].as_str(), r[2]["title"].as_str(), r[2]["placeholder"].as_str()), (Some("item-3"), Some("API key"), Some("sk-")));
    }

    #[test]
    fn an_attachment_with_no_text_is_still_a_message() {
        let p = json!({
            "turnItems": [{
                "id": "1", "type": "user_message", "text": "", "status": "completed",
                "attachments": [{ "type": "file", "name": "notes.txt" }],
            }],
        });
        let t = transcript(&p);
        assert_eq!(t["messages"][0]["attachments"][0]["name"], "notes.txt");
        let command = send_command(
            "thread",
            "",
            &t["messages"][0]["attachments"].as_array().cloned().unwrap(),
        );
        assert_eq!(command["attachments"][0]["name"], "notes.txt");
        assert_eq!(command["text"], "");
    }
}
