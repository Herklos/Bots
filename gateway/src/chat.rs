//! A small REST face on a Bot's main T3 thread for clients that don't speak T3's Effect RPC,
//! such as the Exact app's `app.ts`: read the conversation, send a message, stop a run, answer
//! an approval. Everything else stays reachable through the `/t3/*` passthrough.
use serde_json::{json, Value};

const LIVE: &[&str] = &[
    "running",
    "in_progress",
    "pending",
    "queued",
    "streaming",
    "starting",
];

/// The bubbles and cards of a thread projection, oldest first.
pub fn transcript(projection: &Value) -> Value {
    let items = projection["turnItems"]
        .as_array()
        .or_else(|| projection["visibleTurnItems"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut messages = Vec::new();
    let mut requests = Vec::new();
    let mut working = false;
    for raw in items {
        let item = if raw["item"].is_object() {
            raw["item"].clone()
        } else {
            raw
        };
        let kind = item["type"].as_str().unwrap_or("");
        let status = item["status"].as_str().unwrap_or("");
        let live = LIVE.contains(&status) || item["streaming"] == true;
        match kind {
            "user_message" | "assistant_message" => {
                let text = item["text"].as_str().unwrap_or("").trim().to_string();
                let attachments = item["attachments"].as_array().cloned().unwrap_or_default();
                if text.is_empty() && attachments.is_empty() && !live {
                    continue;
                }
                messages.push(json!({
                    "id": item["id"], "role": if kind == "user_message" { "user" } else { "bot" }, "text": text,
                    "attachments": attachments,
                    "at": item["startedAt"].as_str().or(item["updatedAt"].as_str()).unwrap_or(""), "streaming": live,
                }));
            }
            "approval_request" | "user_input_request" | "secret_request"
                if live || status == "awaiting_response" || status == "waiting" =>
            {
                requests.push(json!({ "id": item["id"], "kind": kind, "title": item["title"].as_str().unwrap_or("The Bot is asking"), "runId": item["runId"] }));
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
    json!({ "messages": messages, "requests": requests, "working": working || active.is_some(), "activeRunId": active })
}

/// The command a phone send dispatches; same fields as hbots `sendMessage`.
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
